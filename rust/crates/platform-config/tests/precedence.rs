//! The documented precedence chain — `env > file > defaults` — plus the
//! local-vs-prod behavioural split and the "no literal secrets" guarantee.
//!
//! Every test injects its own environment map and secret source, so nothing
//! here reads or mutates the real process environment.

mod support;

use std::collections::BTreeMap;

use platform_config::{Config, LayerSource, Loader, MapSecretSource, Profile};
use support::TempDir;

/// Build an environment snapshot from literal pairs.
fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

/// A store holding both credentials the schema declares.
fn stocked_store() -> MapSecretSource {
    MapSecretSource::new()
        .with_env("T_PLAT_DATABASE_PASSWORD", "dummy-db-password")
        .with_env("T_PLAT_MARKET_DATA_API_KEY", "dummy-market-data-key")
}

/// A config directory with a base layer and a `local` profile overlay, each
/// changing a different key so precedence is unambiguous.
fn layered_dir() -> TempDir {
    let dir = TempDir::new("layered");
    dir.write(
        "default.toml",
        "[app]\nname = \"from-base-file\"\n\n[database]\nport = 1111\n\n\
         [market_data]\ntimeout_ms = 1111\n",
    );
    dir.write(
        "local.toml",
        "[database]\nport = 2222\n\n[market_data]\ntimeout_ms = 2222\n",
    );
    dir
}

#[test]
fn layer_1_builtin_defaults_load_with_no_files_and_no_env() {
    let loaded = Loader::from_env_map(BTreeMap::new())
        .config_dir("/nonexistent/config/dir")
        .secret_source(MapSecretSource::new())
        .load()
        .expect("the schema always loads");

    assert_eq!(loaded.config.app, Config::default().app);
    assert_eq!(loaded.config.database.port, 5432);
    assert_eq!(
        loaded.config.profile,
        Profile::Local,
        "local is the default"
    );
    assert_eq!(loaded.sources, vec![LayerSource::BuiltinDefaults]);
}

#[test]
fn layer_2_base_file_beats_builtin_defaults() {
    let dir = layered_dir();
    let loaded = Loader::from_env_map(BTreeMap::new())
        .profile(Profile::Prod) // no prod.toml here, so only default.toml applies
        .config_dir(dir.path())
        .secret_source(stocked_store())
        .load()
        .expect("base file loads");

    assert_eq!(loaded.config.app.name, "from-base-file");
    assert_eq!(loaded.config.database.port, 1111);
    // Keys the base file does not mention still come from the defaults.
    assert_eq!(
        loaded.config.telemetry.otlp_endpoint,
        "http://127.0.0.1:4317"
    );
}

#[test]
fn layer_3_profile_file_beats_base_file() {
    let dir = layered_dir();
    let loaded = Loader::from_env_map(BTreeMap::new())
        .profile(Profile::Local)
        .config_dir(dir.path())
        .secret_source(MapSecretSource::new())
        .load()
        .expect("profile overlay loads");

    assert_eq!(loaded.config.database.port, 2222, "local.toml wins");
    assert_eq!(
        loaded.config.app.name, "from-base-file",
        "keys only the base file sets survive the overlay",
    );
}

#[test]
fn layer_4_explicit_file_beats_the_profile_file() {
    let dir = layered_dir();
    let explicit = dir.write("explicit.toml", "[database]\nport = 3333\n");

    let loaded = Loader::from_env_map(BTreeMap::new())
        .profile(Profile::Local)
        .config_dir(dir.path())
        .config_file(&explicit)
        .secret_source(MapSecretSource::new())
        .load()
        .expect("explicit file loads");

    assert_eq!(loaded.config.database.port, 3333);
    assert_eq!(
        loaded.config.market_data.timeout_ms, 2222,
        "profile layer survives"
    );
}

#[test]
fn layer_5_environment_beats_every_file() {
    let dir = layered_dir();
    let explicit = dir.write("explicit.toml", "[database]\nport = 3333\n");

    let loaded = Loader::from_env_map(env(&[
        ("T_PLAT_PROFILE", "local"),
        ("T_PLAT_CONFIG_FILE", explicit.to_str().expect("utf-8 path")),
        ("T_PLAT__DATABASE__PORT", "4444"),
        ("T_PLAT__MARKET_DATA__TIMEOUT_MS", "4444"),
        ("T_PLAT__APP__NAME", "from-env"),
    ]))
    .config_dir(dir.path())
    .secret_source(MapSecretSource::new())
    .load()
    .expect("env overrides load");

    assert_eq!(loaded.config.database.port, 4444);
    assert_eq!(loaded.config.market_data.timeout_ms, 4444);
    assert_eq!(loaded.config.app.name, "from-env");
}

#[test]
fn the_contributing_layers_are_recorded_in_precedence_order() {
    let dir = layered_dir();
    let explicit = dir.write("explicit.toml", "[database]\nport = 3333\n");

    let loaded = Loader::from_env_map(env(&[("T_PLAT__DATABASE__PORT", "4444")]))
        .profile(Profile::Local)
        .config_dir(dir.path())
        .config_file(&explicit)
        .secret_source(MapSecretSource::new())
        .load()
        .expect("loads");

    assert_eq!(
        loaded.sources,
        vec![
            LayerSource::BuiltinDefaults,
            LayerSource::File(dir.path().join("default.toml")),
            LayerSource::File(dir.path().join("local.toml")),
            LayerSource::File(explicit),
            LayerSource::Environment(vec!["database.port".to_owned()]),
        ],
    );
}

#[test]
fn control_variables_choose_the_profile_and_directory() {
    let dir = layered_dir();
    dir.write("prod.toml", "[app]\nname = \"from-prod-file\"\n");

    let loaded = Loader::from_env_map(env(&[
        ("T_PLAT_PROFILE", "prod"),
        (
            "T_PLAT_CONFIG_DIR",
            dir.path().to_str().expect("utf-8 path"),
        ),
    ]))
    .secret_source(stocked_store())
    .load()
    .expect("control variables are honoured");

    assert_eq!(loaded.config.profile, Profile::Prod);
    assert_eq!(loaded.config.app.name, "from-prod-file");
}

#[test]
fn an_unparseable_profile_is_rejected() {
    let err = Loader::from_env_map(env(&[("T_PLAT_PROFILE", "staging")]))
        .config_dir("/nonexistent")
        .secret_source(MapSecretSource::new())
        .load()
        .expect_err("only local and prod exist");
    assert!(format!("{err}").contains("staging"), "got: {err}");
}

#[test]
fn a_profile_key_inside_a_file_cannot_promote_the_process() {
    let dir = TempDir::new("profile-in-file");
    dir.write("default.toml", "profile = \"prod\"\n");

    let err = Loader::from_env_map(BTreeMap::new())
        .config_dir(dir.path())
        .secret_source(MapSecretSource::new())
        .load()
        .expect_err("`profile` is not a config key");
    assert!(format!("{err}").contains("profile"), "got: {err}");
}

#[test]
fn an_explicitly_requested_file_must_exist() {
    let err = Loader::from_env_map(BTreeMap::new())
        .config_dir("/nonexistent")
        .config_file("/nonexistent/explicit.toml")
        .secret_source(MapSecretSource::new())
        .load()
        .expect_err("explicit means required");
    assert!(format!("{err}").contains("explicit"), "got: {err}");
}

#[test]
fn an_env_override_for_an_undeclared_key_is_an_error() {
    let err = Loader::from_env_map(env(&[("T_PLAT__DATABSE__PORT", "1")]))
        .config_dir("/nonexistent")
        .secret_source(MapSecretSource::new())
        .load()
        .expect_err("typos must not be silently ignored");
    assert!(format!("{err}").contains("databse.port"), "got: {err}");
}

#[test]
fn an_env_override_of_the_wrong_type_is_an_error() {
    let err = Loader::from_env_map(env(&[("T_PLAT__DATABASE__PORT", "not-a-number")]))
        .config_dir("/nonexistent")
        .secret_source(MapSecretSource::new())
        .load()
        .expect_err("overrides stay typed");
    let message = format!("{err}");
    assert!(message.contains("integer"), "got: {message}");
}

#[test]
fn an_out_of_range_value_is_an_error() {
    let err = Loader::from_env_map(env(&[("T_PLAT__TELEMETRY__SAMPLE_RATE", "1.5")]))
        .config_dir("/nonexistent")
        .secret_source(MapSecretSource::new())
        .load()
        .expect_err("sample_rate is a ratio");
    assert!(format!("{err}").contains("sample_rate"), "got: {err}");
}

#[test]
fn an_unknown_key_in_a_file_is_an_error() {
    let dir = TempDir::new("unknown-key");
    dir.write("default.toml", "[app]\nnaem = \"typo\"\n");

    let err = Loader::from_env_map(BTreeMap::new())
        .config_dir(dir.path())
        .secret_source(MapSecretSource::new())
        .load()
        .expect_err("unknown keys are rejected");
    assert!(format!("{err}").contains("naem"), "got: {err}");
}

#[test]
fn a_literal_secret_in_a_config_file_is_rejected() {
    // Written at runtime rather than committed, so the repository itself never
    // contains anything credential-shaped.
    let dir = TempDir::new("literal-secret");
    dir.write(
        "default.toml",
        "[database]\npassword = \"a-literal-instead-of-a-reference\"\n",
    );

    let err = Loader::from_env_map(BTreeMap::new())
        .config_dir(dir.path())
        .secret_source(MapSecretSource::new())
        .load()
        .expect_err("literal secrets must never load");

    let message = format!("{err}");
    assert!(message.contains("literal"), "got: {message}");
    assert!(message.contains("secret reference"), "got: {message}");
}

#[test]
fn local_tolerates_unresolvable_secrets_but_prod_does_not() {
    let empty_store = || MapSecretSource::new();

    let local = Loader::from_env_map(BTreeMap::new())
        .profile(Profile::Local)
        .config_dir("/nonexistent")
        .secret_source(empty_store())
        .load()
        .expect("local boots without credentials");
    assert!(local.secrets.is_empty());

    let err = Loader::from_env_map(BTreeMap::new())
        .profile(Profile::Prod)
        .config_dir("/nonexistent")
        .secret_source(empty_store())
        .load()
        .expect_err("prod fails fast on a missing credential");
    let message = format!("{err}");
    assert!(message.contains("database.password"), "got: {message}");
    assert!(message.contains("prod"), "got: {message}");
}

#[test]
fn prod_resolves_every_declared_secret_when_the_store_is_stocked() {
    let loaded = Loader::from_env_map(BTreeMap::new())
        .profile(Profile::Prod)
        .config_dir("/nonexistent")
        .secret_source(stocked_store())
        .load()
        .expect("prod loads with a stocked store");

    assert_eq!(
        loaded.secrets.keys().collect::<Vec<_>>(),
        ["database.password", "market_data.api_key"],
    );
    assert_eq!(
        loaded.secrets.database_password().map(|s| s.expose()),
        Some("dummy-db-password"),
    );
}

#[test]
fn prod_rejects_debug_diagnostics() {
    let err = Loader::from_env_map(env(&[("T_PLAT__APP__DEBUG", "true")]))
        .profile(Profile::Prod)
        .config_dir("/nonexistent")
        .secret_source(stocked_store())
        .load()
        .expect_err("prod forbids app.debug");
    let message = format!("{err}");
    assert!(message.contains("app.debug"), "got: {message}");
    assert!(message.contains("prod"), "got: {message}");
}

#[test]
fn a_loaders_debug_output_never_leaks_the_environment() {
    let loader = Loader::from_env_map(env(&[("T_PLAT_DATABASE_PASSWORD", "dummy-db-password")]));
    let rendered = format!("{loader:?}");
    assert!(!rendered.contains("dummy-db-password"), "got: {rendered}");
}
