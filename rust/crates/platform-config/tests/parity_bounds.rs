//! Cross-language parity: the Rust half.
//!
//! Every case in `config/testdata/numeric_bounds.toml` (value ranges) and
//! `config/testdata/lexical_overrides.toml` (override string grammar) is driven
//! through the environment-override path here, and through the identical path
//! in `py/tests/test_config_bounds_parity.py`. Both suites assert the same
//! accept/reject outcome per case, so the two implementations of one schema
//! cannot disagree about which configs are valid.

mod support;

use std::collections::BTreeMap;

use platform_config::{
    Config, Loader, MapSecretSource, Profile, MAX_DATABASE_PORT, MAX_MARKET_DATA_MAX_RETRIES,
    MAX_MARKET_DATA_TIMEOUT_MS, MAX_TELEMETRY_SAMPLE_RATE, MIN_DATABASE_PORT,
    MIN_MARKET_DATA_MAX_RETRIES, MIN_MARKET_DATA_TIMEOUT_MS, MIN_TELEMETRY_SAMPLE_RATE,
};
use support::parity_fixture_paths;

/// One parity case as written in a shared fixture.
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
    #[serde(default)]
    expect_bool: Option<bool>,
    #[serde(default)]
    expect_string: Option<String>,
    #[allow(dead_code)]
    why: String,
}

/// A fixture document.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    case: Vec<Case>,
}

/// Every case from every shared fixture.
fn load_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for path in parity_fixture_paths() {
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("{} is readable: {err}", path.display()));
        let fixture = toml::from_str::<Fixture>(&text)
            .unwrap_or_else(|err| panic!("{} is a valid fixture: {err}", path.display()));
        cases.extend(fixture.case);
    }
    cases
}

/// Apply one override to the built-in defaults and report whether it loaded.
///
/// `config_dir` points at nothing, so layer 1 is the only baseline and the
/// case's variable is the single thing under test.
fn load_with_override(case: &Case) -> Result<Config, String> {
    let env: BTreeMap<String, String> = [(case.var.clone(), case.value.clone())].into();
    Loader::from_env_map(env)
        .profile(Profile::Local)
        .config_dir("/nonexistent/config/dir")
        .secret_source(MapSecretSource::new())
        .load()
        .map(|loaded| loaded.config)
        .map_err(|err| err.to_string())
}

/// Check whichever expectation the case declares. Returns a failure message.
fn check_expectations(case: &Case, config: &Config) -> Option<String> {
    let mismatch = |observed: String| {
        Some(format!(
            "`{}`: `{}={:?}` loaded, but the value is {observed}",
            case.name, case.var, case.value
        ))
    };

    if let Some(expected) = case.expect_int {
        let observed = match case.var.as_str() {
            "T_PLAT__DATABASE__PORT" => Some(i64::from(config.database.port)),
            "T_PLAT__MARKET_DATA__TIMEOUT_MS" => i64::try_from(config.market_data.timeout_ms).ok(),
            "T_PLAT__MARKET_DATA__MAX_RETRIES" => Some(i64::from(config.market_data.max_retries)),
            other => return Some(format!("`{}`: no integer reader for `{other}`", case.name)),
        };
        if observed != Some(expected) {
            return mismatch(format!("{observed:?}, expected {expected}"));
        }
    }

    if let Some(expected) = case.expect_float {
        let observed = match case.var.as_str() {
            "T_PLAT__TELEMETRY__SAMPLE_RATE" => config.telemetry.sample_rate,
            other => return Some(format!("`{}`: no float reader for `{other}`", case.name)),
        };
        if (observed - expected).abs() > f64::EPSILON {
            return mismatch(format!("{observed}, expected {expected}"));
        }
    }

    if let Some(expected) = case.expect_bool {
        let observed = match case.var.as_str() {
            "T_PLAT__APP__DEBUG" => config.app.debug,
            other => return Some(format!("`{}`: no boolean reader for `{other}`", case.name)),
        };
        if observed != expected {
            return mismatch(format!("{observed}, expected {expected}"));
        }
    }

    if let Some(expected) = case.expect_string.as_deref() {
        let observed = match case.var.as_str() {
            "T_PLAT__APP__LOG_LEVEL" => config.app.log_level.as_str(),
            other => return Some(format!("`{}`: no string reader for `{other}`", case.name)),
        };
        if observed != expected {
            return mismatch(format!("{observed:?}, expected {expected:?}"));
        }
    }

    None
}

#[test]
fn every_fixture_case_matches_the_declared_outcome() {
    let cases = load_cases();
    assert!(
        cases.len() >= 70,
        "both fixtures should be loaded, found {} cases",
        cases.len()
    );

    let mut failures = Vec::new();

    for case in &cases {
        match (case.accept, load_with_override(case)) {
            (true, Err(err)) => failures.push(format!(
                "`{}`: expected `{}={:?}` to load, but it failed: {err}",
                case.name, case.var, case.value
            )),
            (false, Ok(_)) => failures.push(format!(
                "`{}`: expected `{}={:?}` to be rejected, but it loaded",
                case.name, case.var, case.value
            )),
            (true, Ok(config)) => failures.extend(check_expectations(case, &config)),
            (false, Err(_)) => {}
        }
    }

    assert!(
        failures.is_empty(),
        "cross-language parity failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn the_fixtures_cover_every_overridable_type() {
    let cases = load_cases();
    // Integer, float, boolean and string keys all go through the override
    // coercion, so all four need lexical coverage — not just the numerics.
    for var in [
        "T_PLAT__DATABASE__PORT",
        "T_PLAT__MARKET_DATA__TIMEOUT_MS",
        "T_PLAT__MARKET_DATA__MAX_RETRIES",
        "T_PLAT__TELEMETRY__SAMPLE_RATE",
        "T_PLAT__APP__DEBUG",
        "T_PLAT__APP__LOG_LEVEL",
    ] {
        let covered = cases.iter().filter(|case| case.var == var).count();
        assert!(
            covered >= 2,
            "`{var}` needs parity coverage, found {covered} cases"
        );
    }
}

#[test]
fn the_lexical_fixture_pins_the_known_stdlib_divergences() {
    // These exact spellings are the ones Python's int()/float()/str.strip
    // accept and Rust's parse/trim reject. If a case is ever dropped from the
    // fixture, the class of bug it guards silently comes back.
    let cases = load_cases();
    for value in [
        "1_0",
        "0.5_0",
        "\u{1f}10",
        "\u{661}\u{660}",
        "\u{ff11}\u{ff10}",
    ] {
        assert!(
            cases.iter().any(|case| case.value == value && !case.accept),
            "the fixture must keep a rejecting case for {value:?}",
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
