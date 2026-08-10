"""The typed configuration schema.

These dataclasses are the single Python-side definition of what configuration
exists. Unknown keys are rejected, so a typo in a TOML file is a load error
rather than a silently ignored key, and the Rust mirror in
``rust/crates/platform-config/src/model.rs`` declares the same names and types.

Substrate only: connectivity, logging and telemetry. No trading, pricing or
strategy settings.
"""

from __future__ import annotations

from dataclasses import dataclass, replace
from enum import StrEnum
from typing import Any

from t_plat.config.errors import (
    InvalidProfileError,
    OutOfRangeError,
    ProfileRuleError,
    SchemaError,
    SecretLiteralError,
)
from t_plat.config.secret import SecretPolicy, SecretRef

__all__ = [
    "AppConfig",
    "Config",
    "DatabaseConfig",
    "MarketDataConfig",
    "Profile",
    "TelemetryConfig",
]

#: Log levels the schema accepts, lowest to highest severity.
LOG_LEVELS = ("trace", "debug", "info", "warn", "error")


class Profile(StrEnum):
    """Which deployment profile is active.

    Chosen by the ``T_PLAT_PROFILE`` environment variable only — never by a
    config file — so a file can never promote itself to ``prod``.
    """

    LOCAL = "local"
    """Developer workstation and CI. Permissive secrets, ``debug`` allowed."""

    PROD = "prod"
    """Deployed environments. Required secrets, ``debug`` forbidden."""

    @classmethod
    def parse(cls, raw: str) -> Profile:
        """Parse a profile name, case-insensitively."""
        try:
            return cls(raw.strip().lower())
        except ValueError as exc:
            raise InvalidProfileError(
                f"invalid profile `{raw}`: expected `local` or `prod`"
            ) from exc

    @property
    def secret_policy(self) -> SecretPolicy:
        """How strict this profile is about secrets that do not resolve."""
        return SecretPolicy.PERMISSIVE if self is Profile.LOCAL else SecretPolicy.REQUIRED


def _table(raw: object, path: str) -> dict[str, Any]:
    """Require ``raw`` to be a table."""
    if not isinstance(raw, dict):
        raise SchemaError(f"config key `{path}` must be a table, got {type(raw).__name__}")
    return raw


def _section(table: dict[str, Any], name: str) -> dict[str, Any]:
    """Pull a required section out of ``table``."""
    if name not in table:
        raise SchemaError(f"config is missing the `{name}` section")
    return _table(table[name], name)


def _reject_unknown(table: dict[str, Any], known: tuple[str, ...], path: str) -> None:
    """Fail on any key the schema does not declare."""
    for key in table:
        if key not in known:
            location = f"{path}.{key}" if path else key
            raise SchemaError(
                f"unknown config key `{location}` (expected one of: {', '.join(known)})"
            )


def _string(table: dict[str, Any], key: str, path: str) -> str:
    raw = _required(table, key, path)
    if not isinstance(raw, str):
        raise SchemaError(f"config key `{path}.{key}` must be a string, got {_kind(raw)}")
    return raw


def _integer(table: dict[str, Any], key: str, path: str) -> int:
    raw = _required(table, key, path)
    # bool is a subclass of int in Python; the schema means them separately.
    if isinstance(raw, bool) or not isinstance(raw, int):
        raise SchemaError(f"config key `{path}.{key}` must be an integer, got {_kind(raw)}")
    return raw


def _boolean(table: dict[str, Any], key: str, path: str) -> bool:
    raw = _required(table, key, path)
    if not isinstance(raw, bool):
        raise SchemaError(f"config key `{path}.{key}` must be a boolean, got {_kind(raw)}")
    return raw


def _number(table: dict[str, Any], key: str, path: str) -> float:
    raw = _required(table, key, path)
    if isinstance(raw, bool) or not isinstance(raw, int | float):
        raise SchemaError(f"config key `{path}.{key}` must be a float, got {_kind(raw)}")
    return float(raw)


def _secret_ref(table: dict[str, Any], key: str, path: str) -> SecretRef:
    """Parse a secret-typed field, rejecting committed literals."""
    raw = _required(table, key, path)
    if not isinstance(raw, str):
        raise SchemaError(
            f"config key `{path}.{key}` must be a secret reference string, got {_kind(raw)}"
        )
    try:
        return SecretRef.parse(raw)
    except ValueError as exc:
        raise SecretLiteralError(f"config key `{path}.{key}`: {exc}") from exc


def _required(table: dict[str, Any], key: str, path: str) -> object:
    if key not in table:
        raise SchemaError(f"config is missing key `{path}.{key}`")
    return table[key]


def _kind(raw: object) -> str:
    """Type name used in schema error messages."""
    return "boolean" if isinstance(raw, bool) else type(raw).__name__


@dataclass(frozen=True, slots=True)
class AppConfig:
    """Application-level settings."""

    name: str
    """Service name used in logs and telemetry resource attributes."""

    log_level: str
    """Log verbosity: ``trace`` | ``debug`` | ``info`` | ``warn`` | ``error``."""

    debug: bool
    """Verbose diagnostics. Must be ``False`` under ``prod``."""

    KEYS = ("name", "log_level", "debug")

    @classmethod
    def from_table(cls, table: dict[str, Any]) -> AppConfig:
        """Parse the ``[app]`` section."""
        _reject_unknown(table, cls.KEYS, "app")
        return cls(
            name=_string(table, "name", "app"),
            log_level=_string(table, "log_level", "app"),
            debug=_boolean(table, "debug", "app"),
        )

    def to_table(self) -> dict[str, Any]:
        """Render back to a TOML-shaped table."""
        return {"name": self.name, "log_level": self.log_level, "debug": self.debug}


@dataclass(frozen=True, slots=True)
class DatabaseConfig:
    """Postgres connection settings."""

    host: str
    """Hostname or IP."""

    port: int
    """TCP port."""

    name: str
    """Database name."""

    user: str
    """Role to connect as."""

    password: SecretRef
    """Reference to the password — never the password itself."""

    KEYS = ("host", "port", "name", "user", "password")

    @classmethod
    def from_table(cls, table: dict[str, Any]) -> DatabaseConfig:
        """Parse the ``[database]`` section."""
        _reject_unknown(table, cls.KEYS, "database")
        return cls(
            host=_string(table, "host", "database"),
            port=_integer(table, "port", "database"),
            name=_string(table, "name", "database"),
            user=_string(table, "user", "database"),
            password=_secret_ref(table, "password", "database"),
        )

    def to_table(self) -> dict[str, Any]:
        """Render back to a TOML-shaped table."""
        return {
            "host": self.host,
            "port": self.port,
            "name": self.name,
            "user": self.user,
            "password": self.password.as_string(),
        }


@dataclass(frozen=True, slots=True)
class MarketDataConfig:
    """Market-data transport settings. Connectivity only."""

    endpoint: str
    """Base URL of the market-data service."""

    timeout_ms: int
    """Per-request timeout in milliseconds."""

    max_retries: int
    """Retry attempts after the first failure."""

    api_key: SecretRef
    """Reference to the API key — never the key itself."""

    KEYS = ("endpoint", "timeout_ms", "max_retries", "api_key")

    @classmethod
    def from_table(cls, table: dict[str, Any]) -> MarketDataConfig:
        """Parse the ``[market_data]`` section."""
        _reject_unknown(table, cls.KEYS, "market_data")
        return cls(
            endpoint=_string(table, "endpoint", "market_data"),
            timeout_ms=_integer(table, "timeout_ms", "market_data"),
            max_retries=_integer(table, "max_retries", "market_data"),
            api_key=_secret_ref(table, "api_key", "market_data"),
        )

    def to_table(self) -> dict[str, Any]:
        """Render back to a TOML-shaped table."""
        return {
            "endpoint": self.endpoint,
            "timeout_ms": self.timeout_ms,
            "max_retries": self.max_retries,
            "api_key": self.api_key.as_string(),
        }


@dataclass(frozen=True, slots=True)
class TelemetryConfig:
    """OpenTelemetry export settings."""

    enabled: bool
    """Emit traces and metrics at all."""

    otlp_endpoint: str
    """OTLP gRPC collector endpoint."""

    sample_rate: float
    """Trace sampling ratio in ``0.0..=1.0``."""

    KEYS = ("enabled", "otlp_endpoint", "sample_rate")

    @classmethod
    def from_table(cls, table: dict[str, Any]) -> TelemetryConfig:
        """Parse the ``[telemetry]`` section."""
        _reject_unknown(table, cls.KEYS, "telemetry")
        return cls(
            enabled=_boolean(table, "enabled", "telemetry"),
            otlp_endpoint=_string(table, "otlp_endpoint", "telemetry"),
            sample_rate=_number(table, "sample_rate", "telemetry"),
        )

    def to_table(self) -> dict[str, Any]:
        """Render back to a TOML-shaped table."""
        return {
            "enabled": self.enabled,
            "otlp_endpoint": self.otlp_endpoint,
            "sample_rate": self.sample_rate,
        }


@dataclass(frozen=True, slots=True)
class Config:
    """The fully resolved, typed configuration.

    Holds no secret material: secret-typed fields carry a
    :class:`~t_plat.config.secret.SecretRef`, and the resolved values live
    alongside in :class:`~t_plat.config.secret.ResolvedSecrets`.
    """

    profile: Profile
    """The active profile. Set by the loader from the environment, never parsed
    from a file."""

    app: AppConfig
    """Application-level settings."""

    database: DatabaseConfig
    """Postgres connection settings."""

    market_data: MarketDataConfig
    """Market-data transport settings."""

    telemetry: TelemetryConfig
    """OpenTelemetry export settings."""

    SECTIONS = ("app", "database", "market_data", "telemetry")

    @classmethod
    def defaults(cls) -> Config:
        """Precedence layer 1: the schema always loads, even with no files and
        no environment. Values mirror ``config/default.toml``.
        """
        return cls(
            profile=Profile.LOCAL,
            app=AppConfig(name="t-plat", log_level="info", debug=False),
            database=DatabaseConfig(
                host="127.0.0.1",
                port=5432,
                name="t_plat",
                user="t_plat",
                password=SecretRef.env("T_PLAT_DATABASE_PASSWORD"),
            ),
            market_data=MarketDataConfig(
                endpoint="http://127.0.0.1:8080",
                timeout_ms=5_000,
                max_retries=3,
                api_key=SecretRef.env("T_PLAT_MARKET_DATA_API_KEY"),
            ),
            telemetry=TelemetryConfig(
                enabled=False,
                otlp_endpoint="http://127.0.0.1:4317",
                sample_rate=1.0,
            ),
        )

    @classmethod
    def from_table(cls, table: dict[str, Any], profile: Profile) -> Config:
        """Parse a merged TOML document into the typed schema.

        ``profile`` comes from the loader, not from ``table`` — a ``profile``
        key inside a file is rejected as unknown.
        """
        document = _table(table, "")
        _reject_unknown(document, cls.SECTIONS, "")
        return cls(
            profile=profile,
            app=AppConfig.from_table(_section(document, "app")),
            database=DatabaseConfig.from_table(_section(document, "database")),
            market_data=MarketDataConfig.from_table(_section(document, "market_data")),
            telemetry=TelemetryConfig.from_table(_section(document, "telemetry")),
        )

    def to_table(self) -> dict[str, Any]:
        """Render back to a TOML-shaped document.

        ``profile`` is deliberately omitted, mirroring the Rust schema.
        """
        return {
            "app": self.app.to_table(),
            "database": self.database.to_table(),
            "market_data": self.market_data.to_table(),
            "telemetry": self.telemetry.to_table(),
        }

    def with_profile(self, profile: Profile) -> Config:
        """Return a copy pinned to ``profile``."""
        return replace(self, profile=profile)

    def secret_refs(self) -> list[tuple[str, SecretRef]]:
        """Every secret-typed field, as ``(dotted key, reference)`` pairs.

        This is the registry the ``prod`` fail-fast check walks, and the list a
        new secret field must be added to.
        """
        return [
            ("database.password", self.database.password),
            ("market_data.api_key", self.market_data.api_key),
        ]

    def validate(self) -> None:
        """Check the value constraints the type system cannot express.

        Runs for every profile. Raises :class:`OutOfRangeError` naming the
        offending key.
        """
        if self.app.log_level not in LOG_LEVELS:
            raise OutOfRangeError(
                f"config key `app.log_level` is out of range: "
                f"`{self.app.log_level}` is not one of {', '.join(LOG_LEVELS)}"
            )
        if not self.app.name.strip():
            raise OutOfRangeError("config key `app.name` is out of range: must not be empty")
        if not 1 <= self.database.port <= 65535:
            raise OutOfRangeError(
                "config key `database.port` is out of range: must be in 1..=65535"
            )
        if self.market_data.timeout_ms <= 0:
            raise OutOfRangeError(
                "config key `market_data.timeout_ms` is out of range: must be greater than 0"
            )
        if self.market_data.max_retries < 0:
            raise OutOfRangeError(
                "config key `market_data.max_retries` is out of range: must not be negative"
            )
        if not 0.0 <= self.telemetry.sample_rate <= 1.0:
            raise OutOfRangeError(
                f"config key `telemetry.sample_rate` is out of range: "
                f"{self.telemetry.sample_rate} is outside 0.0..=1.0"
            )

    def validate_profile_rules(self) -> None:
        """Check the rules that only apply under the active profile.

        This is half of the local-vs-prod split (the other half is the secret
        policy): ``prod`` refuses to boot with debug diagnostics on. Raises
        :class:`ProfileRuleError`.
        """
        if self.profile is Profile.PROD and self.app.debug:
            raise ProfileRuleError(
                "config is invalid for the `prod` profile: `app.debug` must be false "
                "(verbose diagnostics leak internals)"
            )
