"""Typed configuration loading and secret sourcing for the platform (S0-T5).

Substrate only — connectivity, logging and telemetry settings. No trading,
pricing or strategy configuration lives here.

This is the Python half of a shared schema; the Rust half is
``rust/crates/platform-config``. Both read the same TOML files in ``config/``
and produce the same typed structures. ``config/README.md`` is the canonical
description of the schema and the split below.

Precedence — ``env > file > defaults``
--------------------------------------

Layers, lowest to highest. Each deep-merges over the one below, so a higher
layer states only what it changes.

1. **Built-in defaults** — :meth:`Config.defaults`. The schema always loads.
2. **Base file** — ``<config_dir>/default.toml`` (optional).
3. **Profile file** — ``<config_dir>/<profile>.toml`` (optional).
4. **Explicit file** — :data:`ENV_CONFIG_FILE` (optional, must exist if set).
5. **Environment** — ``T_PLAT__<SECTION>__<KEY>``, parsed as the type the
   schema declares for that key, under one explicit ASCII grammar
   (``[+-]?[0-9]+`` for integers, and so on) that neither language trims.
   See :mod:`t_plat.config.merge` and the grammar table in ``config/README.md``.

Control variables (:data:`ENV_PROFILE`, :data:`ENV_CONFIG_DIR`,
:data:`ENV_CONFIG_FILE`) use a single underscore and choose *what* to load;
value overrides use the ``T_PLAT__`` double-underscore prefix. A variable
targeting an undeclared key, or an unknown key inside a file, is a load error
rather than a silent no-op.

One schema, two implementations
-------------------------------

``rust/crates/platform-config`` mirrors this package key for key. To keep the
two from diverging, every numeric field's canonical range is declared here as
``MIN_*``/``MAX_*`` constants and mirrored in ``model.rs``, the override
string grammar is spelled out in :mod:`t_plat.config.merge` rather than
delegated to ``int()``/``float()`` (whose leniency differs from Rust's
``str::parse``), and ``config/testdata/{numeric_bounds,lexical_overrides}.toml``
are shared fixtures whose cases both test suites run — asserting the same
accept/reject outcome. Python integers are unbounded and its builtins are more
permissive, so both the ranges and the grammar are enforced explicitly where
Rust gets them from ``u16``/``u32``/``u64`` and ``str::parse``. See the tables
in ``config/README.md``.

Secrets are never in the repo
-----------------------------

Secret-typed fields parse only into a :class:`SecretRef` — ``env:NAME``,
``file:/path`` (absolute), or ``none``. A literal written into the file raises
:class:`SecretLiteralError`. The loader dereferences those pointers after
merging, against a :class:`SecretSource` (the process environment and
secret-store mounts in production; an injected :class:`MappingSecretSource` in
tests). Values are wrapped in :class:`Secret`, which renders as
``Secret(<redacted>)`` everywhere and requires an explicit
:meth:`Secret.expose` call to read.

Local vs prod
-------------

The profile is a behavioural switch, not just different values:

===================  ==========================  ==========================
                     ``Profile.LOCAL``           ``Profile.PROD``
===================  ==========================  ==========================
Secret policy        permissive — unresolved     required — any unresolved
                     secrets are simply absent   secret fails the load
``app.debug = True`` allowed                     rejected
===================  ==========================  ==========================

Example
-------

>>> from t_plat.config import MappingSecretSource, Profile, load_config
>>> loaded = load_config(
...     env={"T_PLAT__DATABASE__PORT": "6543"},
...     profile=Profile.PROD,
...     config_dir="does/not/exist",  # built-in defaults only
...     secret_source=MappingSecretSource(
...         env={
...             "T_PLAT_DATABASE_PASSWORD": "from-the-secret-store",
...             "T_PLAT_MARKET_DATA_API_KEY": "also-from-the-store",
...         }
...     ),
... )
>>> loaded.config.database.port  # env beat the default
6543
>>> secret = loaded.secrets.database_password()
>>> repr(secret)
'Secret(<redacted>)'
>>> secret.expose() if secret else None
'from-the-secret-store'
"""

from __future__ import annotations

from t_plat.config.errors import (
    ConfigError,
    ConfigFileError,
    EnvOverrideError,
    InvalidProfileError,
    MissingSecretError,
    OutOfRangeError,
    ProfileRuleError,
    SchemaError,
    SecretLiteralError,
    SecretStoreError,
)
from t_plat.config.loader import (
    BASE_FILE_NAME,
    DEFAULT_CONFIG_DIR,
    ENV_CONFIG_DIR,
    ENV_CONFIG_FILE,
    ENV_PROFILE,
    ConfigLoader,
    LayerSource,
    LoadedConfig,
    load_config,
)
from t_plat.config.merge import ENV_PATH_SEPARATOR, ENV_VALUE_PREFIX
from t_plat.config.model import (
    LOG_LEVELS,
    MAX_DATABASE_PORT,
    MAX_MARKET_DATA_MAX_RETRIES,
    MAX_MARKET_DATA_TIMEOUT_MS,
    MAX_TELEMETRY_SAMPLE_RATE,
    MIN_DATABASE_PORT,
    MIN_MARKET_DATA_MAX_RETRIES,
    MIN_MARKET_DATA_TIMEOUT_MS,
    MIN_TELEMETRY_SAMPLE_RATE,
    AppConfig,
    Config,
    DatabaseConfig,
    MarketDataConfig,
    Profile,
    TelemetryConfig,
)
from t_plat.config.secret import (
    SECRET_REF_SYNTAX,
    MappingSecretSource,
    OsSecretSource,
    ResolvedSecrets,
    Secret,
    SecretKind,
    SecretPolicy,
    SecretRef,
    SecretSource,
)

__all__ = [
    "BASE_FILE_NAME",
    "DEFAULT_CONFIG_DIR",
    "ENV_CONFIG_DIR",
    "ENV_CONFIG_FILE",
    "ENV_PATH_SEPARATOR",
    "ENV_PROFILE",
    "ENV_VALUE_PREFIX",
    "LOG_LEVELS",
    "MAX_DATABASE_PORT",
    "MAX_MARKET_DATA_MAX_RETRIES",
    "MAX_MARKET_DATA_TIMEOUT_MS",
    "MAX_TELEMETRY_SAMPLE_RATE",
    "MIN_DATABASE_PORT",
    "MIN_MARKET_DATA_MAX_RETRIES",
    "MIN_MARKET_DATA_TIMEOUT_MS",
    "MIN_TELEMETRY_SAMPLE_RATE",
    "SECRET_REF_SYNTAX",
    "AppConfig",
    "Config",
    "ConfigError",
    "ConfigFileError",
    "ConfigLoader",
    "DatabaseConfig",
    "EnvOverrideError",
    "InvalidProfileError",
    "LayerSource",
    "LoadedConfig",
    "MappingSecretSource",
    "MarketDataConfig",
    "MissingSecretError",
    "OsSecretSource",
    "OutOfRangeError",
    "Profile",
    "ProfileRuleError",
    "ResolvedSecrets",
    "SchemaError",
    "Secret",
    "SecretKind",
    "SecretLiteralError",
    "SecretPolicy",
    "SecretRef",
    "SecretSource",
    "SecretStoreError",
    "TelemetryConfig",
    "load_config",
]
