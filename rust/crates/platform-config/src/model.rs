//! The typed configuration schema.
//!
//! These structs are the single Rust-side definition of what configuration
//! exists. Every struct is `deny_unknown_fields`, so a typo in a TOML file is
//! a load error rather than a silently ignored key, and the Python mirror in
//! `py/src/t_plat/config/model.py` declares the same names and types.
//!
//! Substrate only: connectivity, logging and telemetry. No trading, pricing or
//! strategy settings.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::ConfigError;
use crate::secret::{SecretPolicy, SecretRef};

/// Which deployment profile is active.
///
/// Chosen by the `T_PLAT_PROFILE` environment variable only — never by a
/// config file — so a file can never promote itself to `prod`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Profile {
    /// Developer workstation and CI. Permissive secrets, `debug` allowed.
    #[default]
    Local,
    /// Deployed environments. Required secrets, `debug` forbidden.
    Prod,
}

impl Profile {
    /// The profile's lowercase name, as written in `T_PLAT_PROFILE` and in the
    /// `<profile>.toml` filename.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Prod => "prod",
        }
    }

    /// How strict this profile is about secrets that do not resolve.
    #[must_use]
    pub fn secret_policy(self) -> SecretPolicy {
        match self {
            Self::Local => SecretPolicy::Permissive,
            Self::Prod => SecretPolicy::Required,
        }
    }
}

impl FromStr for Profile {
    type Err = ConfigError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "local" => Ok(Self::Local),
            "prod" => Ok(Self::Prod),
            _ => Err(ConfigError::InvalidProfile {
                value: value.to_owned(),
            }),
        }
    }
}

impl fmt::Display for Profile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The fully resolved, typed configuration.
///
/// Produced by [`Loader::load`](crate::Loader::load). Holds no secret
/// material: secret-typed fields carry a [`SecretRef`], and the resolved
/// values live alongside in
/// [`ResolvedSecrets`](crate::ResolvedSecrets).
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// The active profile. Set by the loader from the environment, never
    /// deserialized from a file.
    #[serde(skip)]
    pub profile: Profile,
    /// Application-level settings.
    pub app: AppConfig,
    /// Postgres connection settings.
    pub database: DatabaseConfig,
    /// Market-data transport settings.
    pub market_data: MarketDataConfig,
    /// OpenTelemetry export settings.
    pub telemetry: TelemetryConfig,
}

/// Application-level settings.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppConfig {
    /// Service name used in logs and telemetry resource attributes.
    pub name: String,
    /// Log verbosity: `trace` | `debug` | `info` | `warn` | `error`.
    pub log_level: String,
    /// Verbose diagnostics. Must be `false` under `prod`.
    pub debug: bool,
}

/// Postgres connection settings.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DatabaseConfig {
    /// Hostname or IP.
    pub host: String,
    /// TCP port.
    pub port: u16,
    /// Database name.
    pub name: String,
    /// Role to connect as.
    pub user: String,
    /// Reference to the password — never the password itself.
    pub password: SecretRef,
}

/// Market-data transport settings. Connectivity only.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MarketDataConfig {
    /// Base URL of the market-data service.
    pub endpoint: String,
    /// Per-request timeout in milliseconds.
    pub timeout_ms: u64,
    /// Retry attempts after the first failure.
    pub max_retries: u32,
    /// Reference to the API key — never the key itself.
    pub api_key: SecretRef,
}

/// OpenTelemetry export settings.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TelemetryConfig {
    /// Emit traces and metrics at all.
    pub enabled: bool,
    /// OTLP gRPC collector endpoint.
    pub otlp_endpoint: String,
    /// Trace sampling ratio in `0.0..=1.0`.
    pub sample_rate: f64,
}

/// Log levels the schema accepts, lowest to highest severity.
const LOG_LEVELS: [&str; 5] = ["trace", "debug", "info", "warn", "error"];

// --- Canonical numeric bounds -----------------------------------------------
//
// These are part of the shared cross-language schema: `py/src/t_plat/config/
// model.py` declares the same constants and enforces the same ranges, and
// `config/testdata/numeric_bounds.toml` drives boundary cases through both
// implementations so they cannot drift apart. Changing a bound here means
// changing it there.
//
// Each field has two ranges, mirrored exactly on the Python side:
//
// * *representable* — what the field's integer type can hold. Enforced here by
//   the type itself during deserialization (`ConfigError::Schema`); Python has
//   no fixed-width integers, so it checks this range explicitly.
// * *semantic* — the subset that actually makes sense, enforced by
//   [`Config::validate`] in both languages (`ConfigError::OutOfRange`).

/// Lowest usable TCP port. `0` is representable in a `u16` but never valid.
pub const MIN_DATABASE_PORT: u16 = 1;
/// Highest TCP port, and the top of the `u16` representable range.
pub const MAX_DATABASE_PORT: u16 = u16::MAX;

/// A request timeout must be positive.
pub const MIN_MARKET_DATA_TIMEOUT_MS: u64 = 1;
/// Top of the timeout range.
///
/// The field is a `u64`, but TOML integers are signed 64-bit, so no config
/// file or environment override can express a larger value in either language.
/// Pinning the bound here makes that limit explicit and identical on both
/// sides rather than an accident of the parsers.
pub const MAX_MARKET_DATA_TIMEOUT_MS: u64 = i64::MAX as u64;

/// Retrying zero times is valid: it means "try once, then give up".
pub const MIN_MARKET_DATA_MAX_RETRIES: u32 = 0;
/// Top of the `u32` representable range for retry counts.
pub const MAX_MARKET_DATA_MAX_RETRIES: u32 = u32::MAX;

/// Sample nothing.
pub const MIN_TELEMETRY_SAMPLE_RATE: f64 = 0.0;
/// Sample everything.
pub const MAX_TELEMETRY_SAMPLE_RATE: f64 = 1.0;

impl Default for Config {
    /// Precedence layer 1: the schema always loads, even with no files and no
    /// environment. Values mirror `config/default.toml`.
    fn default() -> Self {
        Self {
            profile: Profile::Local,
            app: AppConfig {
                name: "t-plat".to_owned(),
                log_level: "info".to_owned(),
                debug: false,
            },
            database: DatabaseConfig {
                host: "127.0.0.1".to_owned(),
                port: 5432,
                name: "t_plat".to_owned(),
                user: "t_plat".to_owned(),
                password: SecretRef::Env("T_PLAT_DATABASE_PASSWORD".to_owned()),
            },
            market_data: MarketDataConfig {
                endpoint: "http://127.0.0.1:8080".to_owned(),
                timeout_ms: 5_000,
                max_retries: 3,
                api_key: SecretRef::Env("T_PLAT_MARKET_DATA_API_KEY".to_owned()),
            },
            telemetry: TelemetryConfig {
                enabled: false,
                otlp_endpoint: "http://127.0.0.1:4317".to_owned(),
                sample_rate: 1.0,
            },
        }
    }
}

impl Config {
    /// Every secret-typed field, as `(dotted key, reference)` pairs.
    ///
    /// This is the registry the `prod` fail-fast check walks, and the list a
    /// new secret field must be added to.
    #[must_use]
    pub fn secret_refs(&self) -> Vec<(&'static str, &SecretRef)> {
        vec![
            ("database.password", &self.database.password),
            ("market_data.api_key", &self.market_data.api_key),
        ]
    }

    /// Check the value constraints that the type system cannot express.
    ///
    /// Runs for every profile.
    ///
    /// # Errors
    ///
    /// [`ConfigError::OutOfRange`] naming the offending key.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !LOG_LEVELS.contains(&self.app.log_level.as_str()) {
            return Err(ConfigError::OutOfRange {
                key: "app.log_level",
                reason: format!(
                    "`{}` is not one of {}",
                    self.app.log_level,
                    LOG_LEVELS.join(", ")
                ),
            });
        }
        if self.app.name.trim().is_empty() {
            return Err(ConfigError::OutOfRange {
                key: "app.name",
                reason: "must not be empty".to_owned(),
            });
        }
        // The `u16` type already caps the upper end; only the lower bound needs
        // a runtime check. Python enforces both ends explicitly.
        if self.database.port < MIN_DATABASE_PORT {
            return Err(ConfigError::OutOfRange {
                key: "database.port",
                reason: format!("must be in {MIN_DATABASE_PORT}..={MAX_DATABASE_PORT}"),
            });
        }
        if !(MIN_MARKET_DATA_TIMEOUT_MS..=MAX_MARKET_DATA_TIMEOUT_MS)
            .contains(&self.market_data.timeout_ms)
        {
            return Err(ConfigError::OutOfRange {
                key: "market_data.timeout_ms",
                reason: format!(
                    "{} is outside {MIN_MARKET_DATA_TIMEOUT_MS}..={MAX_MARKET_DATA_TIMEOUT_MS}",
                    self.market_data.timeout_ms
                ),
            });
        }
        // `max_retries` is a `u32`, so both ends of the canonical range are
        // already guaranteed by the type; Python checks them explicitly.
        if !self.telemetry.sample_rate.is_finite() {
            return Err(ConfigError::OutOfRange {
                key: "telemetry.sample_rate",
                reason: format!("{} is not a finite number", self.telemetry.sample_rate),
            });
        }
        if !(MIN_TELEMETRY_SAMPLE_RATE..=MAX_TELEMETRY_SAMPLE_RATE)
            .contains(&self.telemetry.sample_rate)
        {
            return Err(ConfigError::OutOfRange {
                key: "telemetry.sample_rate",
                reason: format!(
                    "{} is outside {MIN_TELEMETRY_SAMPLE_RATE:.1}..={MAX_TELEMETRY_SAMPLE_RATE:.1}",
                    self.telemetry.sample_rate
                ),
            });
        }
        Ok(())
    }

    /// Check the rules that only apply under the active profile.
    ///
    /// This is half of the local-vs-prod split (the other half is the secret
    /// policy): `prod` refuses to boot with debug diagnostics on.
    ///
    /// # Errors
    ///
    /// [`ConfigError::ProfileRule`] describing the violated rule.
    pub fn validate_profile_rules(&self) -> Result<(), ConfigError> {
        if self.profile == Profile::Prod && self.app.debug {
            return Err(ConfigError::ProfileRule {
                profile: Profile::Prod.as_str(),
                reason: "`app.debug` must be false (verbose diagnostics leak internals)".to_owned(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Config, Profile};
    use crate::secret::SecretPolicy;

    #[test]
    fn profile_parses_and_round_trips() {
        assert_eq!("local".parse::<Profile>().expect("local"), Profile::Local);
        assert_eq!("PROD".parse::<Profile>().expect("prod"), Profile::Prod);
        assert_eq!(Profile::Prod.as_str(), "prod");
        assert!("staging".parse::<Profile>().is_err());
    }

    #[test]
    fn profile_selects_the_secret_policy() {
        assert_eq!(Profile::Local.secret_policy(), SecretPolicy::Permissive);
        assert_eq!(Profile::Prod.secret_policy(), SecretPolicy::Required);
    }

    #[test]
    fn defaults_are_valid() {
        Config::default()
            .validate()
            .expect("built-in defaults valid");
    }

    #[test]
    fn every_secret_field_is_registered() {
        let config = Config::default();
        let keys: Vec<_> = config.secret_refs().into_iter().map(|(k, _)| k).collect();
        assert_eq!(keys, ["database.password", "market_data.api_key"]);
    }

    #[test]
    fn out_of_range_values_are_rejected() {
        let mut config = Config::default();
        config.telemetry.sample_rate = 1.5;
        assert!(config.validate().is_err());

        let mut config = Config::default();
        config.app.log_level = "verbose".to_owned();
        assert!(config.validate().is_err());
    }

    #[test]
    fn prod_rejects_debug_but_local_allows_it() {
        let mut config = Config::default();
        config.app.debug = true;

        config.profile = Profile::Local;
        config.validate_profile_rules().expect("local allows debug");

        config.profile = Profile::Prod;
        assert!(config.validate_profile_rules().is_err());
    }
}
