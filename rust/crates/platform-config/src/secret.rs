//! Secret references, redacted secret values, and the sources they come from.
//!
//! The rule this module enforces: **a config file never contains a secret**.
//! It contains a [`SecretRef`] — a pointer to the environment or a secret
//! store — and the loader dereferences it at load time. A literal in a config
//! file is rejected during deserialization, so a committed credential fails
//! the build instead of shipping.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::de::{self, Deserialize, Deserializer};
use serde::ser::{Serialize, Serializer};

use crate::error::ConfigError;

/// The `env:` reference prefix.
const ENV_PREFIX: &str = "env:";
/// The `file:` reference prefix.
const FILE_PREFIX: &str = "file:";
/// The literal spelling of "no secret configured".
const NONE_LITERAL: &str = "none";

/// Human-readable syntax summary, reused in error messages and docs.
pub const SECRET_REF_SYNTAX: &str = "`env:NAME`, `file:/path`, or `none`";

/// A resolved secret value.
///
/// `Debug` and `Display` both render `Secret(<redacted>)`, so a secret cannot
/// reach a log line, a panic message or a serialized error by accident.
/// Reading the material takes an explicit, greppable [`Secret::expose`] call.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// Wrap a value that has already been fetched from a trusted source.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Return the secret material.
    ///
    /// Call this at the point of use (opening a connection, signing a
    /// request) and never store the result.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Length of the secret in bytes. Useful for assertions that must not
    /// touch the value itself.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the secret is the empty string.
    ///
    /// The loader never produces one — an empty environment variable or
    /// secret file counts as unresolved.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

/// Where a secret comes from, as written in a config file.
///
/// This is the only shape a secret-typed field will deserialize from; see the
/// module docs.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum SecretRef {
    /// `"none"` — no secret configured. Allowed under `local`, rejected under
    /// `prod`.
    #[default]
    None,
    /// `"env:NAME"` — read from environment variable `NAME`.
    Env(String),
    /// `"file:/path"` — read from a secret-store mount (Docker/Compose
    /// secrets, Kubernetes secret volumes).
    File(PathBuf),
}

impl SecretRef {
    /// Parse a reference from its textual form.
    ///
    /// # Errors
    ///
    /// Returns the reason the string is not a valid reference. Notably, a
    /// value with no recognised scheme is treated as a committed literal.
    pub fn parse(raw: &str) -> Result<Self, String> {
        if raw == NONE_LITERAL {
            return Ok(Self::None);
        }
        if let Some(name) = raw.strip_prefix(ENV_PREFIX) {
            if name.is_empty() {
                return Err("`env:` reference is missing a variable name".to_owned());
            }
            return Ok(Self::Env(name.to_owned()));
        }
        if let Some(path) = raw.strip_prefix(FILE_PREFIX) {
            if path.is_empty() {
                return Err("`file:` reference is missing a path".to_owned());
            }
            // Checked as a literal leading `/` rather than `Path::is_absolute`
            // so Rust and Python agree on every platform: a secret-store mount
            // is an absolute path, and a relative one would resolve against
            // whatever directory the process happened to start in.
            if !path.starts_with('/') {
                return Err(format!(
                    "`file:` reference `{path}` must be an absolute path \
                     (a secret-store mount, e.g. `file:/run/secrets/name`)"
                ));
            }
            return Ok(Self::File(PathBuf::from(path)));
        }
        Err(format!(
            "expected a secret reference ({SECRET_REF_SYNTAX}), \
             found a literal value — secrets are never committed to config files"
        ))
    }

    /// Render back to the textual form used in config files.
    #[must_use]
    pub fn as_string(&self) -> String {
        match self {
            Self::None => NONE_LITERAL.to_owned(),
            Self::Env(name) => format!("{ENV_PREFIX}{name}"),
            Self::File(path) => format!("{FILE_PREFIX}{}", path.display()),
        }
    }

    /// Resolve the reference against `source` under `policy`.
    ///
    /// `key` is the dotted config key (e.g. `database.password`) and is used
    /// only for error messages.
    ///
    /// # Errors
    ///
    /// Under [`SecretPolicy::Required`] an unresolvable reference is an error.
    /// Under [`SecretPolicy::Permissive`] it yields `Ok(None)`. A `file:`
    /// reference that exists but cannot be read is an error under either
    /// policy — that is a broken mount, not an absent secret.
    pub fn resolve(
        &self,
        key: &str,
        source: &dyn SecretSource,
        policy: SecretPolicy,
    ) -> Result<Option<Secret>, ConfigError> {
        let missing = |reason: String| -> Result<Option<Secret>, ConfigError> {
            match policy {
                SecretPolicy::Permissive => Ok(None),
                SecretPolicy::Required => Err(ConfigError::MissingSecret {
                    key: key.to_owned(),
                    reason,
                }),
            }
        };

        match self {
            Self::None => missing(format!(
                "it is set to `{NONE_LITERAL}` (expected {SECRET_REF_SYNTAX})"
            )),
            Self::Env(name) => match source.env_var(name) {
                Some(value) if !value.is_empty() => Ok(Some(Secret::new(value))),
                _ => missing(format!("environment variable `{name}` is unset or empty")),
            },
            Self::File(path) => match source.read_file(path) {
                Ok(contents) => {
                    // Secret mounts routinely carry a trailing newline.
                    let value = contents.trim_end_matches(['\n', '\r']);
                    if value.is_empty() {
                        missing(format!("secret file `{}` is empty", path.display()))
                    } else {
                        Ok(Some(Secret::new(value)))
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    missing(format!("secret file `{}` does not exist", path.display()))
                }
                Err(source) => Err(ConfigError::SecretFile {
                    key: key.to_owned(),
                    path: path.clone(),
                    source,
                }),
            },
        }
    }
}

impl fmt::Display for SecretRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_string())
    }
}

impl<'de> Deserialize<'de> for SecretRef {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(de::Error::custom)
    }
}

impl Serialize for SecretRef {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.as_string())
    }
}

/// How strict the loader is about secrets that do not resolve.
///
/// Selected by the active [`Profile`](crate::Profile); this is the concrete
/// local-vs-prod behavioural split.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecretPolicy {
    /// `local`: an unresolvable reference yields "unset" so a developer can
    /// boot without provisioning every credential.
    Permissive,
    /// `prod`: every declared secret must resolve or the process refuses to
    /// start. A missing credential is a boot failure, not a runtime surprise.
    Required,
}

/// Where secret material is actually fetched from.
///
/// Implemented by [`OsSecretSource`] for real runs and [`MapSecretSource`] for
/// tests, so the test suite never has to mutate the process environment.
pub trait SecretSource {
    /// Read an environment variable, or `None` if it is unset.
    fn env_var(&self, name: &str) -> Option<String>;

    /// Read a secret-store file.
    ///
    /// # Errors
    ///
    /// Any I/O failure. [`std::io::ErrorKind::NotFound`] is treated as "the
    /// secret is absent"; anything else is a hard error.
    fn read_file(&self, path: &Path) -> std::io::Result<String>;
}

/// The real source: the process environment plus the filesystem.
#[derive(Clone, Copy, Debug, Default)]
pub struct OsSecretSource;

impl SecretSource for OsSecretSource {
    fn env_var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    fn read_file(&self, path: &Path) -> std::io::Result<String> {
        std::fs::read_to_string(path)
    }
}

/// An in-memory secret source for tests and embedding.
///
/// Lets a test exercise `prod`'s required-secret policy — including
/// `file:` references to secret-store mounts that do not exist on the test
/// machine — without touching the real environment or filesystem.
#[derive(Clone, Debug, Default)]
pub struct MapSecretSource {
    env: BTreeMap<String, String>,
    files: BTreeMap<PathBuf, String>,
}

impl MapSecretSource {
    /// Create an empty source: every reference resolves to "absent".
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an environment variable for `env:` references.
    #[must_use]
    pub fn with_env(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(name.into(), value.into());
        self
    }

    /// Register a secret-store file for `file:` references.
    #[must_use]
    pub fn with_file(mut self, path: impl Into<PathBuf>, value: impl Into<String>) -> Self {
        self.files.insert(path.into(), value.into());
        self
    }
}

impl SecretSource for MapSecretSource {
    fn env_var(&self, name: &str) -> Option<String> {
        self.env.get(name).cloned()
    }

    fn read_file(&self, path: &Path) -> std::io::Result<String> {
        self.files.get(path).cloned().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("no secret registered at `{}`", path.display()),
            )
        })
    }
}

/// The secrets resolved for one [`Config`](crate::Config), keyed by dotted
/// config key.
///
/// Holds resolved [`Secret`] values only; keys whose reference did not resolve
/// under a permissive policy are simply absent.
#[derive(Clone, Debug, Default)]
pub struct ResolvedSecrets {
    entries: BTreeMap<String, Secret>,
}

impl ResolvedSecrets {
    /// Insert a resolved secret under its dotted config key.
    pub fn insert(&mut self, key: impl Into<String>, secret: Secret) {
        self.entries.insert(key.into(), secret);
    }

    /// Look a secret up by dotted config key (e.g. `database.password`).
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Secret> {
        self.entries.get(key)
    }

    /// The dotted keys that resolved, in sorted order.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    /// How many secrets resolved.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no secret resolved at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The resolved `database.password`, if any.
    #[must_use]
    pub fn database_password(&self) -> Option<&Secret> {
        self.get("database.password")
    }

    /// The resolved `market_data.api_key`, if any.
    #[must_use]
    pub fn market_data_api_key(&self) -> Option<&Secret> {
        self.get("market_data.api_key")
    }
}

#[cfg(test)]
mod tests {
    use super::{MapSecretSource, Secret, SecretPolicy, SecretRef};
    use std::path::PathBuf;

    #[test]
    fn secret_is_redacted_in_debug_and_display() {
        let secret = Secret::new("super-sensitive");
        assert_eq!(format!("{secret:?}"), "Secret(<redacted>)");
        assert_eq!(format!("{secret}"), "Secret(<redacted>)");
        assert!(!format!("{secret:?} {secret}").contains("sensitive"));
        assert_eq!(secret.expose(), "super-sensitive");
    }

    #[test]
    fn refs_round_trip_through_text() {
        for raw in ["none", "env:MY_VAR", "file:/run/secrets/thing"] {
            let parsed = SecretRef::parse(raw).expect("valid reference");
            assert_eq!(parsed.as_string(), raw);
        }
    }

    #[test]
    fn literal_secret_is_rejected() {
        let err = SecretRef::parse("a-bare-literal").expect_err("literals must be rejected");
        assert!(err.contains("literal"), "unexpected message: {err}");
    }

    #[test]
    fn empty_scheme_bodies_are_rejected() {
        assert!(SecretRef::parse("env:").is_err());
        assert!(SecretRef::parse("file:").is_err());
    }

    #[test]
    fn relative_file_references_are_rejected() {
        let err = SecretRef::parse("file:run/secrets/thing").expect_err("must be absolute");
        assert!(err.contains("absolute"), "unexpected message: {err}");
        assert!(SecretRef::parse("file:/run/secrets/thing").is_ok());
    }

    #[test]
    fn env_ref_resolves_and_trims_file_ref() {
        let source = MapSecretSource::new()
            .with_env("MY_VAR", "from-env")
            .with_file("/run/secrets/thing", "from-file\n");

        let env_ref = SecretRef::Env("MY_VAR".to_owned());
        let resolved = env_ref
            .resolve("a.b", &source, SecretPolicy::Required)
            .expect("resolves")
            .expect("present");
        assert_eq!(resolved.expose(), "from-env");

        let file_ref = SecretRef::File(PathBuf::from("/run/secrets/thing"));
        let resolved = file_ref
            .resolve("a.b", &source, SecretPolicy::Required)
            .expect("resolves")
            .expect("present");
        assert_eq!(resolved.expose(), "from-file");
    }

    #[test]
    fn policy_decides_whether_absence_is_fatal() {
        let empty = MapSecretSource::new();
        let missing = SecretRef::Env("NOPE".to_owned());

        assert!(missing
            .resolve("a.b", &empty, SecretPolicy::Permissive)
            .expect("permissive tolerates absence")
            .is_none());

        assert!(missing
            .resolve("a.b", &empty, SecretPolicy::Required)
            .is_err());
    }

    #[test]
    fn empty_env_var_counts_as_unset() {
        let source = MapSecretSource::new().with_env("BLANK", "");
        assert!(SecretRef::Env("BLANK".to_owned())
            .resolve("a.b", &source, SecretPolicy::Permissive)
            .expect("permissive")
            .is_none());
    }
}
