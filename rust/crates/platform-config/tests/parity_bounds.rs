//! Numeric-bounds parity: the Rust half.
//!
//! Every case in `config/testdata/numeric_bounds.toml` is driven through the
//! environment-override path here, and through the identical path in
//! `py/tests/test_config_bounds_parity.py`. Both suites assert the same
//! accept/reject outcome per case, so the two implementations of one schema
//! cannot disagree about which configs are valid.

mod support;

use std::collections::BTreeMap;

use platform_config::{
    Loader, MapSecretSource, Profile, MAX_DATABASE_PORT, MAX_MARKET_DATA_MAX_RETRIES,
    MAX_MARKET_DATA_TIMEOUT_MS, MAX_TELEMETRY_SAMPLE_RATE, MIN_DATABASE_PORT,
    MIN_MARKET_DATA_MAX_RETRIES, MIN_MARKET_DATA_TIMEOUT_MS, MIN_TELEMETRY_SAMPLE_RATE,
};
use support::bounds_fixture_path;

/// One boundary case as written in the shared fixture.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    name: String,
    var: String,
    value: String,
    accept: bool,
    #[serde(default)]
    expect_int: Option<i64>,
    #[serde(default)]
    expect_float: Option<f64>,
    #[allow(dead_code)]
    why: String,
}

/// The fixture document.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    case: Vec<Case>,
}

fn load_cases() -> Vec<Case> {
    let path = bounds_fixture_path();
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("{} is readable: {err}", path.display()));
    toml::from_str::<Fixture>(&text)
        .unwrap_or_else(|err| panic!("{} is a valid fixture: {err}", path.display()))
        .case
}

/// Apply one override to the built-in defaults and report whether it loaded.
///
/// `config_dir` points at nothing, so layer 1 is the only file-free baseline
/// and the case's variable is the single thing under test.
fn load_with_override(case: &Case) -> Result<platform_config::LoadedConfig, String> {
    let env: BTreeMap<String, String> = [(case.var.clone(), case.value.clone())].into();
    Loader::from_env_map(env)
        .profile(Profile::Local)
        .config_dir("/nonexistent/config/dir")
        .secret_source(MapSecretSource::new())
        .load()
        .map_err(|err| err.to_string())
}

/// The integer value the case targets, for accepted cases.
fn observed_int(config: &platform_config::Config, var: &str) -> Option<i64> {
    match var {
        "T_PLAT__DATABASE__PORT" => Some(i64::from(config.database.port)),
        "T_PLAT__MARKET_DATA__TIMEOUT_MS" => i64::try_from(config.market_data.timeout_ms).ok(),
        "T_PLAT__MARKET_DATA__MAX_RETRIES" => Some(i64::from(config.market_data.max_retries)),
        _ => None,
    }
}

/// The float value the case targets, for accepted cases.
fn observed_float(config: &platform_config::Config, var: &str) -> Option<f64> {
    match var {
        "T_PLAT__TELEMETRY__SAMPLE_RATE" => Some(config.telemetry.sample_rate),
        _ => None,
    }
}

#[test]
fn every_fixture_case_matches_the_declared_outcome() {
    let cases = load_cases();
    assert!(
        cases.len() >= 20,
        "fixture should cover every numeric field"
    );

    let mut failures = Vec::new();

    for case in &cases {
        match (case.accept, load_with_override(case)) {
            (true, Err(err)) => failures.push(format!(
                "`{}`: expected `{}={}` to load, but it failed: {err}",
                case.name, case.var, case.value
            )),
            (false, Ok(_)) => failures.push(format!(
                "`{}`: expected `{}={}` to be rejected, but it loaded",
                case.name, case.var, case.value
            )),
            (true, Ok(loaded)) => {
                if let Some(expected) = case.expect_int {
                    let actual = observed_int(&loaded.config, &case.var);
                    if actual != Some(expected) {
                        failures.push(format!(
                            "`{}`: expected {expected}, observed {actual:?}",
                            case.name
                        ));
                    }
                }
                if let Some(expected) = case.expect_float {
                    let actual = observed_float(&loaded.config, &case.var);
                    if actual.is_none_or(|value| (value - expected).abs() > f64::EPSILON) {
                        failures.push(format!(
                            "`{}`: expected {expected}, observed {actual:?}",
                            case.name
                        ));
                    }
                }
            }
            (false, Err(_)) => {}
        }
    }

    assert!(
        failures.is_empty(),
        "bounds parity failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn the_fixture_covers_every_numeric_field() {
    let cases = load_cases();
    for var in [
        "T_PLAT__DATABASE__PORT",
        "T_PLAT__MARKET_DATA__TIMEOUT_MS",
        "T_PLAT__MARKET_DATA__MAX_RETRIES",
        "T_PLAT__TELEMETRY__SAMPLE_RATE",
    ] {
        let covered = cases.iter().filter(|case| case.var == var).count();
        assert!(
            covered >= 4,
            "`{var}` needs boundary coverage, found {covered} cases"
        );
    }
}

#[test]
fn case_names_are_unique() {
    // Names identify a case in a failure message, so duplicates would hide one.
    let cases = load_cases();
    let mut names: Vec<&str> = cases.iter().map(|case| case.name.as_str()).collect();
    names.sort_unstable();
    let unique = names.len();
    names.dedup();
    assert_eq!(names.len(), unique, "fixture case names must be unique");
}

#[test]
fn the_declared_constants_are_the_ones_python_declares() {
    // Mirrored verbatim in py/src/t_plat/config/model.py. If either side moves
    // a bound without the other, this and its Python twin disagree.
    assert_eq!(MIN_DATABASE_PORT, 1);
    assert_eq!(MAX_DATABASE_PORT, 65_535);
    assert_eq!(MIN_MARKET_DATA_TIMEOUT_MS, 1);
    assert_eq!(MAX_MARKET_DATA_TIMEOUT_MS, 9_223_372_036_854_775_807);
    assert_eq!(MIN_MARKET_DATA_MAX_RETRIES, 0);
    assert_eq!(MAX_MARKET_DATA_MAX_RETRIES, 4_294_967_295);
    assert!((MIN_TELEMETRY_SAMPLE_RATE - 0.0).abs() < f64::EPSILON);
    assert!((MAX_TELEMETRY_SAMPLE_RATE - 1.0).abs() < f64::EPSILON);
}
