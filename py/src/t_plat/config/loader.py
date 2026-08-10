"""The layered loader: defaults → files → environment → secret resolution.

Mirrors ``rust/crates/platform-config/src/loader.rs``.
"""

from __future__ import annotations

import os
import tomllib
from collections.abc import Mapping
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from t_plat.config.errors import ConfigFileError
from t_plat.config.merge import apply_env_overrides, deep_merge
from t_plat.config.model import Config, Profile
from t_plat.config.secret import (
    OsSecretSource,
    ResolvedSecrets,
    SecretSource,
)

__all__ = [
    "BASE_FILE_NAME",
    "DEFAULT_CONFIG_DIR",
    "ENV_CONFIG_DIR",
    "ENV_CONFIG_FILE",
    "ENV_PROFILE",
    "ConfigLoader",
    "LayerSource",
    "LoadedConfig",
    "load_config",
]

#: Environment variable selecting the profile.
ENV_PROFILE = "T_PLAT_PROFILE"
#: Environment variable selecting the directory holding the config files.
ENV_CONFIG_DIR = "T_PLAT_CONFIG_DIR"
#: Environment variable naming an extra file layered above the profile file.
ENV_CONFIG_FILE = "T_PLAT_CONFIG_FILE"
#: Directory searched for ``default.toml`` / ``<profile>.toml`` when
#: :data:`ENV_CONFIG_DIR` is unset. Relative to the working directory.
DEFAULT_CONFIG_DIR = "config"
#: Filename of the base layer inside the config directory.
BASE_FILE_NAME = "default.toml"


@dataclass(frozen=True, slots=True)
class LayerSource:
    """Where one contributing layer came from.

    Recorded in load order (lowest precedence first) so the effective config
    can be explained.
    """

    kind: str
    """``"builtin"``, ``"file"`` or ``"env"``."""

    detail: str
    """The file path, or the comma-separated keys the environment overrode."""

    @classmethod
    def builtin(cls) -> LayerSource:
        """Layer 1: :meth:`Config.defaults`."""
        return cls("builtin", "Config.defaults()")

    @classmethod
    def file(cls, path: Path) -> LayerSource:
        """Layers 2–4: a TOML file that existed and was merged."""
        return cls("file", str(path))

    @classmethod
    def env(cls, keys: list[str]) -> LayerSource:
        """Layer 5: ``T_PLAT__*`` overrides, with the keys they changed."""
        return cls("env", ", ".join(keys))


@dataclass(frozen=True, slots=True)
class LoadedConfig:
    """The result of a successful load."""

    config: Config
    """The typed configuration. Contains secret *references*, never values."""

    secrets: ResolvedSecrets
    """The secret values resolved for this config, keyed by dotted config key."""

    sources: list[LayerSource]
    """The layers that contributed, lowest precedence first."""


@dataclass(frozen=True, slots=True)
class ConfigLoader:
    """Builds a :class:`LoadedConfig` from the documented precedence chain.

    The environment and the secret source are both injected rather than read
    from the process, so tests can cover precedence and the ``prod`` secret
    policy hermetically.
    """

    env: Mapping[str, str] = field(default_factory=dict)
    """The environment snapshot to read control variables and overrides from."""

    profile: Profile | None = None
    """Forces the profile, ignoring :data:`ENV_PROFILE`."""

    config_dir: Path | None = None
    """Forces the config directory, ignoring :data:`ENV_CONFIG_DIR`."""

    config_file: Path | None = None
    """Forces the explicit file layer, ignoring :data:`ENV_CONFIG_FILE`."""

    secret_source: SecretSource = field(default_factory=OsSecretSource)
    """Where secret material is fetched from."""

    def __repr__(self) -> str:
        # The env snapshot may hold secret material; print only its shape.
        return (
            f"ConfigLoader(env_vars={len(self.env)}, profile={self.profile!r}, "
            f"config_dir={self.config_dir!r}, config_file={self.config_file!r})"
        )

    def load(self) -> LoadedConfig:
        """Run the full precedence chain and resolve secrets.

        Raises a :class:`~t_plat.config.errors.ConfigError` subclass on an
        unreadable or malformed file, an invalid profile, an unknown or
        unparseable environment override, a schema violation (including a
        literal secret), an out-of-range value, a profile rule violation, or —
        under ``prod`` — an unresolvable secret.
        """
        profile = self._resolve_profile()
        config_dir = self._resolve_config_dir()
        sources = [LayerSource.builtin()]

        # Layer 1: built-in defaults. Guarantees every schema key exists, so
        # environment overrides always have a declared type to parse against.
        document = Config.defaults().to_table()

        # Layers 2 and 3: base file, then the profile overlay. Both optional —
        # the built-in defaults are a complete config on their own.
        for candidate in (config_dir / BASE_FILE_NAME, config_dir / f"{profile}.toml"):
            if candidate.is_file():
                document = deep_merge(document, _read_toml(candidate))
                sources.append(LayerSource.file(candidate))

        # Layer 4: the explicit file. Requested explicitly, so it must exist.
        explicit = self._resolve_config_file()
        if explicit is not None:
            if not explicit.is_file():
                raise ConfigFileError(
                    f"config file `{explicit}` was requested explicitly but does not exist"
                )
            document = deep_merge(document, _read_toml(explicit))
            sources.append(LayerSource.file(explicit))

        # Layer 5: typed environment overrides beat every file.
        overridden = apply_env_overrides(document, self.env)
        if overridden:
            sources.append(LayerSource.env(overridden))

        config = Config.from_table(document, profile)
        config.validate()
        config.validate_profile_rules()

        # Secrets live outside the merge chain: the layers only ever carried
        # references, and this is where they are dereferenced.
        policy = profile.secret_policy
        entries = {}
        for key, reference in config.secret_refs():
            secret = reference.resolve(key, self.secret_source, policy)
            if secret is not None:
                entries[key] = secret

        return LoadedConfig(config=config, secrets=ResolvedSecrets(entries), sources=sources)

    def _resolve_profile(self) -> Profile:
        """Builder override, else :data:`ENV_PROFILE`, else ``local``."""
        if self.profile is not None:
            return self.profile
        raw = self.env.get(ENV_PROFILE)
        return Profile.LOCAL if raw is None else Profile.parse(raw)

    def _resolve_config_dir(self) -> Path:
        """Builder override, else :data:`ENV_CONFIG_DIR`, else ``./config``."""
        if self.config_dir is not None:
            return self.config_dir
        return Path(self.env.get(ENV_CONFIG_DIR, DEFAULT_CONFIG_DIR))

    def _resolve_config_file(self) -> Path | None:
        """Builder override, else :data:`ENV_CONFIG_FILE`, else no extra layer."""
        if self.config_file is not None:
            return self.config_file
        raw = self.env.get(ENV_CONFIG_FILE)
        return None if raw is None else Path(raw)


def _read_toml(path: Path) -> dict[str, Any]:
    """Read and parse a TOML file that must exist."""
    try:
        text = path.read_text(encoding="utf-8")
    except OSError as exc:
        raise ConfigFileError(f"cannot read config file `{path}`: {exc}") from exc
    try:
        return tomllib.loads(text)
    except tomllib.TOMLDecodeError as exc:
        raise ConfigFileError(f"cannot parse config file `{path}`: {exc}") from exc


def load_config(
    *,
    env: Mapping[str, str] | None = None,
    profile: Profile | None = None,
    config_dir: Path | str | None = None,
    config_file: Path | str | None = None,
    secret_source: SecretSource | None = None,
) -> LoadedConfig:
    """Load configuration using the documented precedence chain.

    With no arguments this reads the real process environment and ``./config``.
    Every argument is an override for testing or embedding.
    """
    return ConfigLoader(
        env=os.environ if env is None else env,
        profile=profile,
        config_dir=None if config_dir is None else Path(config_dir),
        config_file=None if config_file is None else Path(config_file),
        secret_source=OsSecretSource() if secret_source is None else secret_source,
    ).load()
