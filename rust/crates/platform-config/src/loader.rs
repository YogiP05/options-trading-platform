//! The layered loader: defaults → files → environment → secret resolution.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use toml::Value;

use crate::error::ConfigError;
use crate::merge::{apply_env_overrides, merge_into};
use crate::model::{Config, Profile};
use crate::secret::{OsSecretSource, ResolvedSecrets, SecretSource};

/// Environment variable selecting the profile.
pub const ENV_PROFILE: &str = "T_PLAT_PROFILE";
/// Environment variable selecting the directory holding the config files.
pub const ENV_CONFIG_DIR: &str = "T_PLAT_CONFIG_DIR";
/// Environment variable naming an extra file layered above the profile file.
pub const ENV_CONFIG_FILE: &str = "T_PLAT_CONFIG_FILE";
/// Directory searched for `default.toml` / `<profile>.toml` when
/// [`ENV_CONFIG_DIR`] is unset. Relative to the process working directory.
pub const DEFAULT_CONFIG_DIR: &str = "config";
/// Filename of the base layer inside the config directory.
pub const BASE_FILE_NAME: &str = "default.toml";

/// Where one contributing layer came from. Recorded in load order (lowest
/// precedence first) so the effective config can be explained.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayerSource {
    /// Layer 1: [`Config::default`].
    BuiltinDefaults,
    /// Layers 2–4: a TOML file that existed and was merged.
    File(PathBuf),
    /// Layer 5: `T_PLAT__*` overrides, with the dotted keys they changed.
    Environment(Vec<String>),
}

/// The result of a successful load.
#[derive(Clone, Debug)]
pub struct LoadedConfig {
    /// The typed configuration. Contains secret *references*, never values.
    pub config: Config,
    /// The secret values resolved for this config, keyed by dotted config key.
    pub secrets: ResolvedSecrets,
    /// The layers that contributed, lowest precedence first.
    pub sources: Vec<LayerSource>,
}

/// Builds a [`LoadedConfig`] from the documented precedence chain.
///
/// The environment and the secret source are both injected rather than read
/// from the process, so tests can cover precedence and the `prod` secret
/// policy hermetically — see the crate-level docs for an example.
pub struct Loader {
    env: BTreeMap<String, String>,
    profile: Option<Profile>,
    config_dir: Option<PathBuf>,
    config_file: Option<PathBuf>,
    secret_source: Box<dyn SecretSource>,
}

impl std::fmt::Debug for Loader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The env snapshot may hold secret material; print only its shape.
        f.debug_struct("Loader")
            .field("env_vars", &self.env.len())
            .field("profile", &self.profile)
            .field("config_dir", &self.config_dir)
            .field("config_file", &self.config_file)
            .finish_non_exhaustive()
    }
}

impl Loader {
    /// Build a loader from an explicit environment snapshot and an in-memory
    /// secret source (i.e. nothing is read from the process).
    #[must_use]
    pub fn from_env_map(env: BTreeMap<String, String>) -> Self {
        Self {
            env,
            profile: None,
            config_dir: None,
            config_file: None,
            secret_source: Box::new(OsSecretSource),
        }
    }

    /// Build a loader from the real process environment.
    #[must_use]
    pub fn from_os_env() -> Self {
        Self::from_env_map(std::env::vars().collect())
    }

    /// Force the profile, ignoring [`ENV_PROFILE`].
    #[must_use]
    pub fn profile(mut self, profile: Profile) -> Self {
        self.profile = Some(profile);
        self
    }

    /// Force the config directory, ignoring [`ENV_CONFIG_DIR`].
    #[must_use]
    pub fn config_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.config_dir = Some(dir.into());
        self
    }

    /// Force the explicit config file layer, ignoring [`ENV_CONFIG_FILE`].
    ///
    /// The file must exist; an explicit request is never silently skipped.
    #[must_use]
    pub fn config_file(mut self, file: impl Into<PathBuf>) -> Self {
        self.config_file = Some(file.into());
        self
    }

    /// Resolve secrets against `source` instead of the process environment and
    /// filesystem.
    #[must_use]
    pub fn secret_source(mut self, source: impl SecretSource + 'static) -> Self {
        self.secret_source = Box::new(source);
        self
    }

    /// Run the full precedence chain and resolve secrets.
    ///
    /// # Errors
    ///
    /// Any [`ConfigError`]: an unreadable or malformed file, an invalid
    /// profile, an unknown or unparseable environment override, a schema
    /// violation (including a literal secret), an out-of-range value, a
    /// profile rule violation, or — under `prod` — an unresolvable secret.
    pub fn load(&self) -> Result<LoadedConfig, ConfigError> {
        let profile = self.resolve_profile()?;
        let config_dir = self.resolve_config_dir();
        let mut sources = vec![LayerSource::BuiltinDefaults];

        // Layer 1: built-in defaults. Guarantees every schema key exists, so
        // environment overrides always have a declared type to parse against.
        let mut document = builtin_defaults_document()?;

        // Layers 2 and 3: base file, then the profile overlay. Both optional —
        // the built-in defaults are a complete config on their own.
        for candidate in [
            config_dir.join(BASE_FILE_NAME),
            config_dir.join(format!("{profile}.toml")),
        ] {
            if let Some(layer) = read_optional_toml(&candidate)? {
                merge_into(&mut document, layer);
                sources.push(LayerSource::File(candidate));
            }
        }

        // Layer 4: the explicit file. Requested explicitly, so it must exist.
        if let Some(path) = self.resolve_config_file() {
            if !path.is_file() {
                return Err(ConfigError::MissingExplicitFile { path });
            }
            merge_into(&mut document, read_toml(&path)?);
            sources.push(LayerSource::File(path));
        }

        // Layer 5: typed environment overrides beat every file.
        let overridden = apply_env_overrides(&mut document, &self.env)?;
        if !overridden.is_empty() {
            sources.push(LayerSource::Environment(overridden));
        }

        let mut config = deserialize(&document)?;
        config.profile = profile;
        config.validate()?;
        config.validate_profile_rules()?;

        // Secrets live outside the merge chain: the layers only ever carried
        // references, and this is where they are dereferenced.
        let policy = profile.secret_policy();
        let mut secrets = ResolvedSecrets::default();
        for (key, reference) in config.secret_refs() {
            if let Some(secret) = reference.resolve(key, self.secret_source.as_ref(), policy)? {
                secrets.insert(key, secret);
            }
        }

        Ok(LoadedConfig {
            config,
            secrets,
            sources,
        })
    }

    /// Builder override, else [`ENV_PROFILE`], else [`Profile::Local`].
    fn resolve_profile(&self) -> Result<Profile, ConfigError> {
        if let Some(profile) = self.profile {
            return Ok(profile);
        }
        match self.env.get(ENV_PROFILE) {
            Some(raw) => raw.parse(),
            None => Ok(Profile::default()),
        }
    }

    /// Builder override, else [`ENV_CONFIG_DIR`], else [`DEFAULT_CONFIG_DIR`].
    fn resolve_config_dir(&self) -> PathBuf {
        self.config_dir
            .clone()
            .or_else(|| self.env.get(ENV_CONFIG_DIR).map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_DIR))
    }

    /// Builder override, else [`ENV_CONFIG_FILE`], else no explicit layer.
    fn resolve_config_file(&self) -> Option<PathBuf> {
        self.config_file
            .clone()
            .or_else(|| self.env.get(ENV_CONFIG_FILE).map(PathBuf::from))
    }
}

/// Layer 1 as a TOML document.
fn builtin_defaults_document() -> Result<Value, ConfigError> {
    let rendered = toml::to_string(&Config::default()).map_err(|err| ConfigError::Schema {
        message: format!("built-in defaults are not serializable: {err}"),
    })?;
    toml::from_str::<Value>(&rendered).map_err(|err| ConfigError::Schema {
        message: format!("built-in defaults are not valid TOML: {err}"),
    })
}

/// Read and parse a TOML file that must exist.
fn read_toml(path: &Path) -> Result<Value, ConfigError> {
    let text = std::fs::read_to_string(path).map_err(|source| ConfigError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    toml::from_str::<Value>(&text).map_err(|source| ConfigError::ParseFile {
        path: path.to_path_buf(),
        source,
    })
}

/// Read and parse a TOML file, or `Ok(None)` if it is simply absent.
fn read_optional_toml(path: &Path) -> Result<Option<Value>, ConfigError> {
    if path.is_file() {
        read_toml(path).map(Some)
    } else {
        Ok(None)
    }
}

/// Turn the merged document into the typed schema.
///
/// Goes back through text so the deserializer's error message names the
/// offending key (which is how a literal secret gets reported usefully).
fn deserialize(document: &Value) -> Result<Config, ConfigError> {
    let rendered = toml::to_string(document).map_err(|err| ConfigError::Schema {
        message: format!("merged config is not serializable: {err}"),
    })?;
    toml::from_str::<Config>(&rendered).map_err(|err| ConfigError::Schema {
        message: err.to_string(),
    })
}
