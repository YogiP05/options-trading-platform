"""The documented precedence chain — ``env > file > defaults`` — plus the
local-vs-prod behavioural split and the "no literal secrets" guarantee.

Every test injects its own environment mapping and secret source, so nothing
here reads or mutates the real process environment.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from t_plat.config import (
    Config,
    ConfigFileError,
    ConfigLoader,
    EnvOverrideError,
    InvalidProfileError,
    MappingSecretSource,
    MissingSecretError,
    OutOfRangeError,
    Profile,
    ProfileRuleError,
    SchemaError,
    Secret,
    SecretLiteralError,
    load_config,
)

EMPTY_STORE = MappingSecretSource()
STOCKED_STORE = MappingSecretSource(
    env={
        "T_PLAT_DATABASE_PASSWORD": "dummy-db-password",
        "T_PLAT_MARKET_DATA_API_KEY": "dummy-market-data-key",
    }
)
NO_SUCH_DIR = Path("/nonexistent/config/dir")


@pytest.fixture
def layered_dir(tmp_path: Path) -> Path:
    """A config directory with a base layer and a ``local`` overlay, each
    changing a different key so precedence is unambiguous.
    """
    (tmp_path / "default.toml").write_text(
        '[app]\nname = "from-base-file"\n\n[database]\nport = 1111\n\n'
        "[market_data]\ntimeout_ms = 1111\n",
        encoding="utf-8",
    )
    (tmp_path / "local.toml").write_text(
        "[database]\nport = 2222\n\n[market_data]\ntimeout_ms = 2222\n",
        encoding="utf-8",
    )
    return tmp_path


def test_layer_1_builtin_defaults_load_with_no_files_and_no_env() -> None:
    loaded = load_config(env={}, config_dir=NO_SUCH_DIR, secret_source=EMPTY_STORE)

    assert loaded.config.app == Config.defaults().app
    assert loaded.config.database.port == 5432
    assert loaded.config.profile is Profile.LOCAL, "local is the default"
    assert [source.kind for source in loaded.sources] == ["builtin"]


def test_layer_2_base_file_beats_builtin_defaults(layered_dir: Path) -> None:
    # No prod.toml in this directory, so only default.toml applies.
    loaded = load_config(
        env={}, profile=Profile.PROD, config_dir=layered_dir, secret_source=STOCKED_STORE
    )

    assert loaded.config.app.name == "from-base-file"
    assert loaded.config.database.port == 1111
    # Keys the base file does not mention still come from the defaults.
    assert loaded.config.telemetry.otlp_endpoint == "http://127.0.0.1:4317"


def test_layer_3_profile_file_beats_base_file(layered_dir: Path) -> None:
    loaded = load_config(
        env={}, profile=Profile.LOCAL, config_dir=layered_dir, secret_source=EMPTY_STORE
    )

    assert loaded.config.database.port == 2222, "local.toml wins"
    assert loaded.config.app.name == "from-base-file", (
        "keys only the base file sets survive the overlay"
    )


def test_layer_4_explicit_file_beats_the_profile_file(layered_dir: Path) -> None:
    explicit = layered_dir / "explicit.toml"
    explicit.write_text("[database]\nport = 3333\n", encoding="utf-8")

    loaded = load_config(
        env={},
        profile=Profile.LOCAL,
        config_dir=layered_dir,
        config_file=explicit,
        secret_source=EMPTY_STORE,
    )

    assert loaded.config.database.port == 3333
    assert loaded.config.market_data.timeout_ms == 2222, "profile layer survives"


def test_layer_5_environment_beats_every_file(layered_dir: Path) -> None:
    explicit = layered_dir / "explicit.toml"
    explicit.write_text("[database]\nport = 3333\n", encoding="utf-8")

    loaded = load_config(
        env={
            "T_PLAT_PROFILE": "local",
            "T_PLAT_CONFIG_FILE": str(explicit),
            "T_PLAT__DATABASE__PORT": "4444",
            "T_PLAT__MARKET_DATA__TIMEOUT_MS": "4444",
            "T_PLAT__APP__NAME": "from-env",
        },
        config_dir=layered_dir,
        secret_source=EMPTY_STORE,
    )

    assert loaded.config.database.port == 4444
    assert loaded.config.market_data.timeout_ms == 4444
    assert loaded.config.app.name == "from-env"


def test_the_contributing_layers_are_recorded_in_precedence_order(layered_dir: Path) -> None:
    explicit = layered_dir / "explicit.toml"
    explicit.write_text("[database]\nport = 3333\n", encoding="utf-8")

    loaded = load_config(
        env={"T_PLAT__DATABASE__PORT": "4444"},
        profile=Profile.LOCAL,
        config_dir=layered_dir,
        config_file=explicit,
        secret_source=EMPTY_STORE,
    )

    assert [(source.kind, source.detail) for source in loaded.sources] == [
        ("builtin", "Config.defaults()"),
        ("file", str(layered_dir / "default.toml")),
        ("file", str(layered_dir / "local.toml")),
        ("file", str(explicit)),
        ("env", "database.port"),
    ]


def test_control_variables_choose_the_profile_and_directory(layered_dir: Path) -> None:
    (layered_dir / "prod.toml").write_text('[app]\nname = "from-prod-file"\n', encoding="utf-8")

    loaded = load_config(
        env={"T_PLAT_PROFILE": "prod", "T_PLAT_CONFIG_DIR": str(layered_dir)},
        secret_source=STOCKED_STORE,
    )

    assert loaded.config.profile is Profile.PROD
    assert loaded.config.app.name == "from-prod-file"


def test_an_unparseable_profile_is_rejected() -> None:
    with pytest.raises(InvalidProfileError, match="staging"):
        load_config(
            env={"T_PLAT_PROFILE": "staging"}, config_dir=NO_SUCH_DIR, secret_source=EMPTY_STORE
        )


def test_a_profile_key_inside_a_file_cannot_promote_the_process(tmp_path: Path) -> None:
    (tmp_path / "default.toml").write_text('profile = "prod"\n', encoding="utf-8")

    with pytest.raises(SchemaError, match="profile"):
        load_config(env={}, config_dir=tmp_path, secret_source=EMPTY_STORE)


def test_an_explicitly_requested_file_must_exist() -> None:
    with pytest.raises(ConfigFileError, match="explicit"):
        load_config(
            env={},
            config_dir=NO_SUCH_DIR,
            config_file=NO_SUCH_DIR / "explicit.toml",
            secret_source=EMPTY_STORE,
        )


def test_an_env_override_for_an_undeclared_key_is_an_error() -> None:
    with pytest.raises(EnvOverrideError, match="databse.port"):
        load_config(
            env={"T_PLAT__DATABSE__PORT": "1"}, config_dir=NO_SUCH_DIR, secret_source=EMPTY_STORE
        )


def test_an_env_override_of_the_wrong_type_is_an_error() -> None:
    with pytest.raises(EnvOverrideError, match="integer"):
        load_config(
            env={"T_PLAT__DATABASE__PORT": "not-a-number"},
            config_dir=NO_SUCH_DIR,
            secret_source=EMPTY_STORE,
        )


def test_an_out_of_range_value_is_an_error() -> None:
    with pytest.raises(OutOfRangeError, match="sample_rate"):
        load_config(
            env={"T_PLAT__TELEMETRY__SAMPLE_RATE": "1.5"},
            config_dir=NO_SUCH_DIR,
            secret_source=EMPTY_STORE,
        )


def test_an_unknown_key_in_a_file_is_an_error(tmp_path: Path) -> None:
    (tmp_path / "default.toml").write_text('[app]\nnaem = "typo"\n', encoding="utf-8")

    with pytest.raises(SchemaError, match="naem"):
        load_config(env={}, config_dir=tmp_path, secret_source=EMPTY_STORE)


def test_a_literal_secret_in_a_config_file_is_rejected(tmp_path: Path) -> None:
    # Written at runtime rather than committed, so the repository itself never
    # contains anything credential-shaped.
    (tmp_path / "default.toml").write_text(
        '[database]\npassword = "a-literal-instead-of-a-reference"\n', encoding="utf-8"
    )

    with pytest.raises(SecretLiteralError, match="literal value"):
        load_config(env={}, config_dir=tmp_path, secret_source=EMPTY_STORE)


def test_local_tolerates_unresolvable_secrets_but_prod_does_not() -> None:
    local = load_config(
        env={}, profile=Profile.LOCAL, config_dir=NO_SUCH_DIR, secret_source=EMPTY_STORE
    )
    assert len(local.secrets) == 0

    with pytest.raises(MissingSecretError, match="database.password"):
        load_config(env={}, profile=Profile.PROD, config_dir=NO_SUCH_DIR, secret_source=EMPTY_STORE)


def test_prod_resolves_every_declared_secret_when_the_store_is_stocked() -> None:
    loaded = load_config(
        env={}, profile=Profile.PROD, config_dir=NO_SUCH_DIR, secret_source=STOCKED_STORE
    )

    assert loaded.secrets.keys() == ["database.password", "market_data.api_key"]
    password = loaded.secrets.database_password()
    assert password is not None
    assert password.expose() == "dummy-db-password"


def test_prod_rejects_debug_diagnostics() -> None:
    with pytest.raises(ProfileRuleError, match="app.debug"):
        load_config(
            env={"T_PLAT__APP__DEBUG": "true"},
            profile=Profile.PROD,
            config_dir=NO_SUCH_DIR,
            secret_source=STOCKED_STORE,
        )


def test_a_secret_is_redacted_everywhere_but_expose() -> None:
    secret = Secret("super-sensitive")
    assert repr(secret) == "Secret(<redacted>)"
    assert str(secret) == "Secret(<redacted>)"
    assert "sensitive" not in f"{secret!r} {secret}"
    assert secret.expose() == "super-sensitive"


def test_a_loaders_repr_never_leaks_the_environment() -> None:
    loader = ConfigLoader(env={"T_PLAT_DATABASE_PASSWORD": "dummy-db-password"})
    assert "dummy-db-password" not in repr(loader)
