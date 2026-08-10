//! The committed sample config and the committed profile files are loaded here
//! against the real schema, so neither can drift from the code.

mod support;

use std::collections::BTreeMap;

use platform_config::{Config, Loader, MapSecretSource, Profile, SecretRef};
use support::{example_config_path, flatten_keys, repo_config_dir, repo_root};

/// A secret source stocked with the credentials the committed configs
/// reference. Values are obvious dummies — the point is that the *repo* holds
/// only the references, and the values arrive from outside it.
fn secret_store() -> MapSecretSource {
    MapSecretSource::new()
        .with_env("T_PLAT_DATABASE_PASSWORD", "dummy-db-password")
        .with_env("T_PLAT_MARKET_DATA_API_KEY", "dummy-market-data-key")
        .with_file(
            "/run/secrets/t_plat_market_data_api_key",
            "dummy-mounted-key\n",
        )
}

/// A loader that reads nothing from the process environment.
fn hermetic() -> Loader {
    Loader::from_env_map(BTreeMap::new()).secret_source(secret_store())
}

#[test]
fn example_config_loads_into_the_typed_schema() {
    let loaded = hermetic()
        .profile(Profile::Local)
        .config_dir(repo_root().join("does-not-exist"))
        .config_file(example_config_path())
        .load()
        .expect("config/config.example.toml loads");

    let config = &loaded.config;
    assert_eq!(config.profile, Profile::Local);
    assert_eq!(config.app.name, "t-plat");
    assert_eq!(config.app.log_level, "debug");
    assert!(config.app.debug);
    assert_eq!(config.database.host, "127.0.0.1");
    assert_eq!(config.database.port, 5432);
    assert_eq!(config.database.name, "t_plat");
    assert_eq!(config.database.user, "t_plat");
    assert_eq!(config.market_data.endpoint, "http://127.0.0.1:8080");
    assert_eq!(config.market_data.timeout_ms, 7_500);
    assert_eq!(config.market_data.max_retries, 2);
    assert!(!config.telemetry.enabled);
    assert_eq!(config.telemetry.otlp_endpoint, "http://127.0.0.1:4317");
    assert!((config.telemetry.sample_rate - 1.0).abs() < f64::EPSILON);
}

#[test]
fn example_config_holds_secret_references_never_values() {
    let loaded = hermetic()
        .profile(Profile::Local)
        .config_dir(repo_root().join("does-not-exist"))
        .config_file(example_config_path())
        .load()
        .expect("example config loads");

    assert_eq!(
        loaded.config.database.password,
        SecretRef::Env("T_PLAT_DATABASE_PASSWORD".to_owned()),
    );
    assert_eq!(
        loaded.config.market_data.api_key,
        SecretRef::Env("T_PLAT_MARKET_DATA_API_KEY".to_owned()),
    );

    // The values themselves came from the injected store, not from the file.
    let raw = std::fs::read_to_string(example_config_path()).expect("example is readable");
    for secret in loaded.secrets.keys() {
        let value = loaded.secrets.get(secret).expect("just listed").expose();
        assert!(
            !raw.contains(value),
            "secret material for `{secret}` must not appear in the committed example",
        );
    }
}

#[test]
fn every_committed_config_file_declares_references_only() {
    // If any committed file held a literal credential, the schema would refuse
    // to deserialize it and these loads would fail.
    let dir = repo_config_dir();
    let mut checked = 0;

    for entry in std::fs::read_dir(&dir).expect("config dir is readable") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("config file is readable");
        let document = toml::from_str::<toml::Value>(&text).expect("config file is valid TOML");

        for (table, key) in [("database", "password"), ("market_data", "api_key")] {
            let Some(raw) = document.get(table).and_then(|t| t.get(key)) else {
                continue;
            };
            let raw = raw.as_str().expect("secret fields are strings");
            SecretRef::parse(raw).unwrap_or_else(|err| {
                panic!(
                    "{}: {table}.{key} is not a secret reference: {err}",
                    path.display()
                )
            });
            checked += 1;
        }
    }

    assert!(
        checked >= 4,
        "expected several secret fields, checked {checked}"
    );
}

#[test]
fn example_config_covers_exactly_the_schema() {
    let rendered = toml::to_string(&Config::default()).expect("defaults serialize");
    let defaults = toml::from_str::<toml::Value>(&rendered).expect("defaults parse");
    let example_text = std::fs::read_to_string(example_config_path()).expect("example is readable");
    let example = toml::from_str::<toml::Value>(&example_text).expect("example is valid TOML");

    let mut schema_keys = Vec::new();
    flatten_keys(&defaults, "", &mut schema_keys);
    let mut example_keys = Vec::new();
    flatten_keys(&example, "", &mut example_keys);

    assert_eq!(
        example_keys, schema_keys,
        "config/config.example.toml must document exactly the schema's keys",
    );
}

#[test]
fn env_example_documents_every_referenced_secret_variable() {
    let env_example =
        std::fs::read_to_string(repo_root().join(".env.example")).expect(".env.example exists");

    for profile in [Profile::Local, Profile::Prod] {
        let loaded = hermetic()
            .profile(profile)
            .config_dir(repo_config_dir())
            .load()
            .unwrap_or_else(|err| panic!("committed `{profile}` config loads: {err}"));

        for (key, reference) in loaded.config.secret_refs() {
            if let SecretRef::Env(name) = reference {
                assert!(
                    env_example.contains(name.as_str()),
                    ".env.example must document `{name}` (referenced by `{key}` under `{profile}`)",
                );
            }
        }
    }
}

#[test]
fn committed_local_profile_loads_without_any_secrets() {
    let loaded = Loader::from_env_map(BTreeMap::new())
        .profile(Profile::Local)
        .config_dir(repo_config_dir())
        .secret_source(MapSecretSource::new()) // nothing provisioned at all
        .load()
        .expect("`local` boots without credentials");

    assert_eq!(loaded.config.app.log_level, "debug");
    assert!(loaded.config.app.debug, "local.toml turns debug on");
    assert_eq!(loaded.config.market_data.timeout_ms, 10_000);
    assert!(
        loaded.secrets.is_empty(),
        "no secrets were provisioned, so none resolved",
    );
}

#[test]
fn committed_prod_profile_loads_with_secrets_from_the_store() {
    let loaded = hermetic()
        .profile(Profile::Prod)
        .config_dir(repo_config_dir())
        .load()
        .expect("`prod` loads once the store is stocked");

    assert!(!loaded.config.app.debug, "prod forbids debug");
    assert_eq!(loaded.config.database.host, "db.internal");
    assert_eq!(
        loaded.config.market_data.endpoint,
        "https://market-data.internal"
    );
    assert_eq!(loaded.config.market_data.timeout_ms, 2_000);
    assert!(loaded.config.telemetry.enabled);

    // prod sources the API key from a secret-store mount, not an env var.
    assert_eq!(
        loaded.config.market_data.api_key,
        SecretRef::File("/run/secrets/t_plat_market_data_api_key".into()),
    );
    assert_eq!(
        loaded.secrets.market_data_api_key().map(|s| s.expose()),
        Some("dummy-mounted-key"),
        "trailing newline from the mount is trimmed",
    );
    assert_eq!(
        loaded.secrets.database_password().map(|s| s.expose()),
        Some("dummy-db-password"),
    );
    assert_eq!(loaded.secrets.len(), 2, "every declared secret resolved");
}
