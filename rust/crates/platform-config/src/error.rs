//! Error type for configuration loading.

use std::path::PathBuf;

/// Everything that can go wrong while loading configuration.
///
/// Every variant names the offending key, file or environment variable so a
/// misconfigured deployment fails with an actionable message rather than a
/// silent fallback.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigError {
    /// A config file exists but could not be read.
    #[error("cannot read config file `{path}`: {source}")]
    ReadFile {
        /// The file we tried to read.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// A config file is not valid TOML.
    #[error("cannot parse config file `{path}`: {source}")]
    ParseFile {
        /// The file we tried to parse.
        path: PathBuf,
        /// The underlying TOML error.
        #[source]
        source: toml::de::Error,
    },

    /// `T_PLAT_CONFIG_FILE` (or [`Loader::config_file`]) named a file that is
    /// not there. Explicit means required — we never silently skip it.
    ///
    /// [`Loader::config_file`]: crate::Loader::config_file
    #[error("config file `{path}` was requested explicitly but does not exist")]
    MissingExplicitFile {
        /// The file that was requested.
        path: PathBuf,
    },

    /// `T_PLAT_PROFILE` held something other than `local` or `prod`.
    #[error("invalid profile `{value}`: expected `local` or `prod`")]
    InvalidProfile {
        /// The rejected value.
        value: String,
    },

    /// An environment override targets a key the schema does not define,
    /// which is almost always a typo.
    #[error("environment variable `{var}` targets unknown config key `{key}`")]
    UnknownEnvKey {
        /// The environment variable that was set.
        var: String,
        /// The dotted config key it resolved to.
        key: String,
    },

    /// An environment override could not be parsed as the type the schema
    /// declares for that key.
    #[error("environment variable `{var}` is not a valid {expected} for config key `{key}` (got `{value}`){hint}")]
    InvalidEnvValue {
        /// The environment variable that was set.
        var: String,
        /// The dotted config key it targets.
        key: String,
        /// The type the schema declares for that key.
        expected: &'static str,
        /// The value we failed to parse.
        value: String,
        /// Guidance appended to the message, or empty. Overrides are not
        /// trimmed (see `merge.rs`), so the common near-miss of a stray space
        /// is called out rather than left as a bare "not a valid integer".
        hint: String,
    },

    /// The merged document does not match the schema (unknown key, wrong
    /// type, or a literal where a secret reference was required).
    #[error("config does not match the schema: {message}")]
    Schema {
        /// The deserializer's message, which names the offending key.
        message: String,
    },

    /// A secret reference could not be resolved under the `prod` profile's
    /// required-secret policy.
    #[error("secret `{key}` is required under the `prod` profile but {reason}")]
    MissingSecret {
        /// The dotted config key holding the secret reference.
        key: String,
        /// Why it could not be resolved.
        reason: String,
    },

    /// A `file:` secret reference pointed at something unreadable.
    #[error("cannot read secret file `{path}` for config key `{key}`: {source}")]
    SecretFile {
        /// The dotted config key holding the secret reference.
        key: String,
        /// The secret-store path we tried to read.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// The merged config violates a rule the active profile enforces.
    #[error("config is invalid for the `{profile}` profile: {reason}")]
    ProfileRule {
        /// The active profile.
        profile: &'static str,
        /// The rule that was violated.
        reason: String,
    },

    /// A value is outside the range the schema allows.
    #[error("config key `{key}` is out of range: {reason}")]
    OutOfRange {
        /// The dotted config key.
        key: &'static str,
        /// The constraint that was violated.
        reason: String,
    },
}
