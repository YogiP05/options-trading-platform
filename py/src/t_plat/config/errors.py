"""Errors raised while loading configuration.

Every error names the offending key, file or environment variable so a
misconfigured deployment fails with an actionable message rather than a silent
fallback. The hierarchy mirrors ``ConfigError`` in the Rust crate
``rust/crates/platform-config``.
"""

from __future__ import annotations

__all__ = [
    "ConfigError",
    "ConfigFileError",
    "EnvOverrideError",
    "InvalidProfileError",
    "MissingSecretError",
    "OutOfRangeError",
    "ProfileRuleError",
    "SchemaError",
    "SecretLiteralError",
    "SecretStoreError",
]


class ConfigError(Exception):
    """Base class for every configuration failure."""


class ConfigFileError(ConfigError):
    """A config file could not be read, parsed, or was required but absent."""


class InvalidProfileError(ConfigError):
    """``T_PLAT_PROFILE`` held something other than ``local`` or ``prod``."""


class EnvOverrideError(ConfigError):
    """A ``T_PLAT__*`` override targets an unknown key or has the wrong type."""


class SchemaError(ConfigError):
    """The merged document does not match the schema."""


class SecretLiteralError(SchemaError):
    """A config file holds a literal secret instead of a reference.

    This is the check that keeps credentials out of the repository: it fires
    during parsing, so a committed secret fails the build.
    """


class MissingSecretError(ConfigError):
    """A declared secret did not resolve under the ``prod`` required policy."""


class SecretStoreError(ConfigError):
    """A ``file:`` reference pointed at something that exists but is unreadable."""


class ProfileRuleError(ConfigError):
    """The config violates a rule the active profile enforces."""


class OutOfRangeError(ConfigError):
    """A value is outside the range the schema allows."""
