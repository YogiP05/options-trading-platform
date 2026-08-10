//! Typed configuration loading and secret sourcing for the platform (S0-T5).
//!
//! Substrate only — connectivity, logging and telemetry settings. No trading,
//! pricing or strategy configuration lives here.
//!
//! # Precedence: `env > file > defaults`
//!
//! Layers, lowest to highest. Each deep-merges over the one below, so a higher
//! layer states only what it changes.
//!
//! 1. **Built-in defaults** — [`Config::default`]. The schema always loads.
//! 2. **Base file** — `<config_dir>/default.toml` (optional).
//! 3. **Profile file** — `<config_dir>/<profile>.toml` (optional).
//! 4. **Explicit file** — [`ENV_CONFIG_FILE`] (optional, but must exist if set).
//! 5. **Environment** — `T_PLAT__<SECTION>__<KEY>`, parsed as the type the
//!    schema declares for that key, under one explicit ASCII grammar
//!    (`[+-]?[0-9]+` for integers, and so on) that neither language trims.
//!    See `merge.rs` and the grammar table in `config/README.md`.
//!
//! Control variables ([`ENV_PROFILE`], [`ENV_CONFIG_DIR`], [`ENV_CONFIG_FILE`])
//! use a single underscore and choose *what* to load; value overrides use the
//! `T_PLAT__` double-underscore prefix. A variable targeting an undeclared key,
//! or an unknown key inside a file, is a load error rather than a silent no-op.
//!
//! # One schema, two implementations
//!
//! `py/src/t_plat/config` mirrors this crate key for key. To keep the two from
//! diverging, every numeric field's canonical range is declared here as
//! `MIN_*`/`MAX_*` constants and mirrored in `model.py`, the override string
//! grammar is spelled out in `merge.rs` rather than delegated to `str::parse`
//! (whose leniency differs from Python's `int()`/`float()`), and
//! `config/testdata/{numeric_bounds,lexical_overrides}.toml` are shared
//! fixtures whose cases both test suites run — asserting the same
//! accept/reject outcome. See the tables in `config/README.md`.
//!
//! # Secrets are never in the repo
//!
//! Secret-typed fields deserialize only into a [`SecretRef`] — `env:NAME`,
//! `file:/path` (absolute), or `none`. A literal written into the file fails to load.
//! The loader dereferences those pointers after merging, against a
//! [`SecretSource`] (the process environment and secret-store mounts in
//! production; an injected [`MapSecretSource`] in tests). Values are wrapped in
//! [`Secret`], which renders as `Secret(<redacted>)` everywhere and requires an
//! explicit [`Secret::expose`] call to read.
//!
//! # Local vs prod
//!
//! The profile is a behavioural switch, not just different values:
//!
//! | | [`Profile::Local`] | [`Profile::Prod`] |
//! |---|---|---|
//! | Secret policy | [`Permissive`](SecretPolicy::Permissive) — unresolved secrets are simply absent | [`Required`](SecretPolicy::Required) — any unresolved secret fails the load |
//! | `app.debug = true` | allowed | rejected |
//!
//! # Example
//!
//! ```
//! use platform_config::{Loader, MapSecretSource, Profile};
//! use std::collections::BTreeMap;
//!
//! let env: BTreeMap<String, String> =
//!     [("T_PLAT__DATABASE__PORT".to_owned(), "6543".to_owned())].into();
//!
//! let loaded = Loader::from_env_map(env)
//!     .profile(Profile::Prod)
//!     .config_dir("does/not/exist") // built-in defaults only
//!     .secret_source(
//!         MapSecretSource::new()
//!             .with_env("T_PLAT_DATABASE_PASSWORD", "from-the-secret-store")
//!             .with_env("T_PLAT_MARKET_DATA_API_KEY", "also-from-the-store"),
//!     )
//!     .load()
//!     .expect("config loads");
//!
//! assert_eq!(loaded.config.database.port, 6543); // env beat the default
//! assert_eq!(
//!     loaded.secrets.database_password().map(|s| s.expose()),
//!     Some("from-the-secret-store"),
//! );
//! assert_eq!(format!("{:?}", loaded.secrets.database_password().unwrap()), "Secret(<redacted>)");
//! ```
//!
//! See `config/README.md` at the repo root for the shared schema and the
//! Python mirror of this loader.

mod error;
mod loader;
mod merge;
mod model;
mod secret;

pub use error::ConfigError;
pub use loader::{
    LayerSource, LoadedConfig, Loader, BASE_FILE_NAME, DEFAULT_CONFIG_DIR, ENV_CONFIG_DIR,
    ENV_CONFIG_FILE, ENV_PROFILE,
};
pub use model::{
    AppConfig, Config, DatabaseConfig, MarketDataConfig, Profile, TelemetryConfig,
    MAX_DATABASE_PORT, MAX_MARKET_DATA_MAX_RETRIES, MAX_MARKET_DATA_TIMEOUT_MS,
    MAX_TELEMETRY_SAMPLE_RATE, MIN_DATABASE_PORT, MIN_MARKET_DATA_MAX_RETRIES,
    MIN_MARKET_DATA_TIMEOUT_MS, MIN_TELEMETRY_SAMPLE_RATE,
};
pub use secret::{
    MapSecretSource, OsSecretSource, ResolvedSecrets, Secret, SecretPolicy, SecretRef,
    SecretSource, SECRET_REF_SYNTAX,
};
