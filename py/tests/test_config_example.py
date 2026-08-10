"""The committed sample config and profile files are loaded here against the
real Python schema, so neither can drift from the code — nor from the Rust
mirror, which asserts the same values in
``rust/crates/platform-config/tests/example_config.rs``.
"""

from __future__ import annotations

import tomllib
from pathlib import Path
from typing import Any

from t_plat.config import (
    Config,
    MappingSecretSource,
    Profile,
    SecretRef,
    load_config,
)

REPO_ROOT = Path(__file__).resolve().parents[2]
CONFIG_DIR = REPO_ROOT / "config"
EXAMPLE_CONFIG = CONFIG_DIR / "config.example.toml"
ENV_EXAMPLE = REPO_ROOT / ".env.example"

# Obvious dummies. The point is that the *repo* holds only references, and the
# values arrive from outside it.
STOCKED_STORE = MappingSecretSource(
    env={
        "T_PLAT_DATABASE_PASSWORD": "dummy-db-password",
        "T_PLAT_MARKET_DATA_API_KEY": "dummy-market-data-key",
    },
    files={"/run/secrets/t_plat_market_data_api_key": "dummy-mounted-key\n"},
)


def _flatten(value: Any, prefix: str = "") -> list[str]:
    """Flatten a TOML document into sorted dotted keys."""
    if not isinstance(value, dict):
        return [prefix]
    keys: list[str] = []
    for key, child in value.items():
        keys.extend(_flatten(child, f"{prefix}.{key}" if prefix else key))
    return sorted(keys)


def test_example_config_loads_into_the_typed_schema() -> None:
    loaded = load_config(
        env={},
        profile=Profile.LOCAL,
        config_dir=REPO_ROOT / "does-not-exist",
        config_file=EXAMPLE_CONFIG,
        secret_source=STOCKED_STORE,
    )

    config = loaded.config
    assert config.profile is Profile.LOCAL
    assert config.app.name == "t-plat"
    assert config.app.log_level == "debug"
    assert config.app.debug is True
    assert config.database.host == "127.0.0.1"
    assert config.database.port == 5432
    assert config.database.name == "t_plat"
    assert config.database.user == "t_plat"
    assert config.market_data.endpoint == "http://127.0.0.1:8080"
    assert config.market_data.timeout_ms == 7500
    assert config.market_data.max_retries == 2
    assert config.telemetry.enabled is False
    assert config.telemetry.otlp_endpoint == "http://127.0.0.1:4317"
    assert config.telemetry.sample_rate == 1.0


def test_example_config_holds_secret_references_never_values() -> None:
    loaded = load_config(
        env={},
        profile=Profile.LOCAL,
        config_dir=REPO_ROOT / "does-not-exist",
        config_file=EXAMPLE_CONFIG,
        secret_source=STOCKED_STORE,
    )

    assert loaded.config.database.password == SecretRef.env("T_PLAT_DATABASE_PASSWORD")
    assert loaded.config.market_data.api_key == SecretRef.env("T_PLAT_MARKET_DATA_API_KEY")

    # The values themselves came from the injected store, not from the file.
    raw = EXAMPLE_CONFIG.read_text(encoding="utf-8")
    for key in loaded.secrets.keys():
        secret = loaded.secrets.get(key)
        assert secret is not None
        assert secret.expose() not in raw, f"secret material for `{key}` must not be committed"


def test_every_committed_config_file_declares_references_only() -> None:
    # If any committed file held a literal credential, the schema would refuse
    # to parse it and every load in this module would fail.
    checked = 0
    for path in sorted(CONFIG_DIR.glob("*.toml")):
        document = tomllib.loads(path.read_text(encoding="utf-8"))
        for table, key in (("database", "password"), ("market_data", "api_key")):
            raw = document.get(table, {}).get(key)
            if raw is None:
                continue
            assert isinstance(raw, str)
            SecretRef.parse(raw)  # raises ValueError on a literal
            checked += 1

    assert checked >= 4, f"expected several secret fields, checked {checked}"


def test_example_config_covers_exactly_the_schema() -> None:
    example = tomllib.loads(EXAMPLE_CONFIG.read_text(encoding="utf-8"))
    assert _flatten(example) == _flatten(Config.defaults().to_table()), (
        "config/config.example.toml must document exactly the schema's keys"
    )


def _dotenv_assignment_keys(text: str) -> set[str]:
    """The ``KEY`` of every ``KEY=value`` line in a dotenv-style file.

    Blank lines, comments and any ``export `` prefix are ignored. Parsed rather
    than substring-matched so a variable that survives only inside a comment
    does not satisfy the drift check. Mirrors ``dotenv_assignment_keys`` in the
    Rust test support module.
    """
    keys: set[str] = set()
    for raw_line in text.splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key = line.split("=", 1)[0].strip().removeprefix("export ").strip()
        if key:
            keys.add(key)
    return keys


def test_env_example_documents_every_referenced_secret_variable() -> None:
    assigned = _dotenv_assignment_keys(ENV_EXAMPLE.read_text(encoding="utf-8"))
    assert assigned, ".env.example must contain real assignments, not only commentary"

    for profile in (Profile.LOCAL, Profile.PROD):
        loaded = load_config(
            env={},
            profile=profile,
            config_dir=CONFIG_DIR,
            secret_source=STOCKED_STORE,
        )
        for key, reference in loaded.config.secret_refs():
            if reference.kind.value == "env":
                assert reference.target in assigned, (
                    f".env.example must assign `{reference.target}` "
                    f"(referenced by `{key}` under `{profile}`); "
                    f"assignments found: {sorted(assigned)}"
                )


def test_committed_local_profile_loads_without_any_secrets() -> None:
    loaded = load_config(
        env={},
        profile=Profile.LOCAL,
        config_dir=CONFIG_DIR,
        secret_source=MappingSecretSource(),  # nothing provisioned at all
    )

    assert loaded.config.app.log_level == "debug"
    assert loaded.config.app.debug is True, "local.toml turns debug on"
    assert loaded.config.market_data.timeout_ms == 10000
    assert len(loaded.secrets) == 0, "no secrets were provisioned, so none resolved"


def test_committed_prod_profile_loads_with_secrets_from_the_store() -> None:
    loaded = load_config(
        env={},
        profile=Profile.PROD,
        config_dir=CONFIG_DIR,
        secret_source=STOCKED_STORE,
    )

    assert loaded.config.app.debug is False, "prod forbids debug"
    assert loaded.config.database.host == "db.internal"
    assert loaded.config.market_data.endpoint == "https://market-data.internal"
    assert loaded.config.market_data.timeout_ms == 2000
    assert loaded.config.telemetry.enabled is True

    # prod sources the API key from a secret-store mount, not an env var.
    assert loaded.config.market_data.api_key == SecretRef.file(
        "/run/secrets/t_plat_market_data_api_key"
    )
    api_key = loaded.secrets.market_data_api_key()
    assert api_key is not None
    assert api_key.expose() == "dummy-mounted-key", "trailing newline from the mount is trimmed"

    password = loaded.secrets.database_password()
    assert password is not None
    assert password.expose() == "dummy-db-password"
    assert len(loaded.secrets) == 2, "every declared secret resolved"


def test_both_languages_agree_on_the_committed_prod_config() -> None:
    """Guards the shared-schema claim in config/README.md.

    These are the same assertions the Rust integration test makes; if the two
    loaders ever diverge on the committed files, one of the suites fails.
    """
    loaded = load_config(
        env={"T_PLAT_PROFILE": "prod", "T_PLAT__DATABASE__PORT": "6543"},
        config_dir=CONFIG_DIR,
        secret_source=STOCKED_STORE,
    )

    assert loaded.config.profile is Profile.PROD
    assert loaded.config.database.port == 6543, "env beats the committed file"
    assert loaded.config.telemetry.sample_rate == 0.1
    assert loaded.secrets.keys() == ["database.password", "market_data.api_key"]
