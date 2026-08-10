//! Numerical golden-fixture recording and comparison.
//!
//! Tests normally compare computed values with a committed JSON fixture. Set
//! `UPDATE_GOLDEN=1` only when an intentional numerical change should replace
//! and re-approve that fixture.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// Environment variable that enables fixture recording.
pub const UPDATE_GOLDEN_ENV: &str = "UPDATE_GOLDEN";

/// A mismatch between a computed numerical output and its golden fixture.
#[derive(Debug)]
pub struct GoldenError {
    fixture: PathBuf,
    detail: String,
}

impl fmt::Display for GoldenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "golden comparison failed for {}: {}. If this change is intentional, review it and run `just golden-update`",
            self.fixture.display(),
            self.detail
        )
    }
}

impl std::error::Error for GoldenError {}

/// Compare `actual` with a line-oriented fixture, or update the fixture when
/// `UPDATE_GOLDEN=1`.
///
/// Each element must be finite. A comparison passes when its absolute error is
/// less than or equal to `tolerance`. The tolerance is supplied by the caller
/// so each future oracle suite can choose and document an appropriate value.
///
/// # Errors
///
/// Returns an error for invalid inputs, fixture I/O or decoding failures,
/// length differences, and values outside the configured tolerance.
pub fn assert_vector(
    fixture_path: impl AsRef<Path>,
    actual: &[f64],
    tolerance: f64,
) -> Result<(), GoldenError> {
    let fixture_path = fixture_path.as_ref();
    validate_inputs(fixture_path, actual, tolerance)?;

    if update_requested() {
        return record_fixture(fixture_path, actual);
    }

    let encoded = fs::read_to_string(fixture_path).map_err(|error| GoldenError {
        fixture: fixture_path.to_path_buf(),
        detail: format!("could not read fixture: {error}"),
    })?;
    let expected = parse_fixture(fixture_path, &encoded)?;

    if expected.len() != actual.len() {
        return Err(GoldenError {
            fixture: fixture_path.to_path_buf(),
            detail: format!(
                "length mismatch: expected {}, got {}",
                expected.len(),
                actual.len()
            ),
        });
    }

    for (index, (&expected, &actual)) in expected.iter().zip(actual).enumerate() {
        let error = (actual - expected).abs();
        // The negation is intentional: unlike `error > tolerance`, this rejects NaN.
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        if !(error <= tolerance) {
            return Err(GoldenError {
                fixture: fixture_path.to_path_buf(),
                detail: format!(
                    "value {index} differs: expected {expected:.17}, got {actual:.17}, absolute error {error:.3e} exceeds tolerance {tolerance:.3e}"
                ),
            });
        }
    }

    Ok(())
}

fn parse_fixture(path: &Path, encoded: &str) -> Result<Vec<f64>, GoldenError> {
    encoded
        .lines()
        .enumerate()
        .map(|(index, line)| {
            let value = line.parse::<f64>().map_err(|error| GoldenError {
                fixture: path.to_path_buf(),
                detail: format!("fixture line {} is not a number: {error}", index + 1),
            })?;
            if !value.is_finite() {
                return Err(GoldenError {
                    fixture: path.to_path_buf(),
                    detail: format!("fixture value {} must be finite, got {value}", index + 1),
                });
            }
            Ok(value)
        })
        .collect()
}

fn update_requested() -> bool {
    std::env::var(UPDATE_GOLDEN_ENV).is_ok_and(|value| value == "1")
}

fn validate_inputs(path: &Path, actual: &[f64], tolerance: f64) -> Result<(), GoldenError> {
    if !tolerance.is_finite() || tolerance < 0.0 {
        return Err(GoldenError {
            fixture: path.to_path_buf(),
            detail: format!("tolerance must be finite and non-negative, got {tolerance}"),
        });
    }
    if let Some((index, value)) = actual
        .iter()
        .enumerate()
        .find(|(_, value)| !value.is_finite())
    {
        return Err(GoldenError {
            fixture: path.to_path_buf(),
            detail: format!("actual value {index} must be finite, got {value}"),
        });
    }
    Ok(())
}

fn record_fixture(path: &Path, actual: &[f64]) -> Result<(), GoldenError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| GoldenError {
            fixture: path.to_path_buf(),
            detail: format!("could not create fixture directory: {error}"),
        })?;
    }
    let encoded = actual
        .iter()
        .map(|value| format!("{value:.17}\n"))
        .collect::<String>();
    fs::write(path, encoded).map_err(|error| GoldenError {
        fixture: path.to_path_buf(),
        detail: format!("could not write fixture: {error}"),
    })
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Mutex, MutexGuard};

    use super::{assert_vector, UPDATE_GOLDEN_ENV};

    static NEXT_FIXTURE_ID: AtomicU64 = AtomicU64::new(0);
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct TestEnvironment {
        _lock: MutexGuard<'static, ()>,
        previous_update: Option<OsString>,
    }

    impl TestEnvironment {
        fn without_update() -> Self {
            let lock = ENV_LOCK.lock().unwrap();
            let previous_update = std::env::var_os(UPDATE_GOLDEN_ENV);
            std::env::remove_var(UPDATE_GOLDEN_ENV);
            Self {
                _lock: lock,
                previous_update,
            }
        }
    }

    impl Drop for TestEnvironment {
        fn drop(&mut self) {
            match self.previous_update.take() {
                Some(value) => std::env::set_var(UPDATE_GOLDEN_ENV, value),
                None => std::env::remove_var(UPDATE_GOLDEN_ENV),
            }
        }
    }

    fn temporary_fixture(contents: &str) -> PathBuf {
        let id = NEXT_FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("golden-test-self-tests")
            .join(format!("{}-{id}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("fixture.golden");
        fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn value_mismatch_beyond_tolerance_errors() {
        let _environment = TestEnvironment::without_update();
        let fixture = temporary_fixture("1.0\n");

        let error = assert_vector(fixture, &[1.01], 0.001).unwrap_err();

        assert!(error.to_string().contains("exceeds tolerance"));
    }

    #[test]
    fn length_mismatch_errors() {
        let _environment = TestEnvironment::without_update();
        let fixture = temporary_fixture("1.0\n2.0\n");

        let error = assert_vector(fixture, &[1.0], 0.0).unwrap_err();

        assert!(error.to_string().contains("length mismatch"));
    }

    #[test]
    fn non_finite_fixture_values_error() {
        let _environment = TestEnvironment::without_update();

        for value in ["NaN", "inf", "-inf"] {
            let fixture = temporary_fixture(&format!("{value}\n"));
            let error = assert_vector(fixture, &[1.0], 0.0).unwrap_err();
            assert!(error.to_string().contains("must be finite"));
        }
    }

    #[test]
    fn invalid_tolerances_error() {
        let _environment = TestEnvironment::without_update();
        let fixture = temporary_fixture("1.0\n");

        for tolerance in [-1.0, f64::NAN] {
            let error = assert_vector(&fixture, &[1.0], tolerance).unwrap_err();
            assert!(error
                .to_string()
                .contains("tolerance must be finite and non-negative"));
        }
    }

    #[test]
    fn update_environment_rewrites_fixture() {
        let _environment = TestEnvironment::without_update();
        let fixture = temporary_fixture("99.0\n");
        std::env::set_var(UPDATE_GOLDEN_ENV, "1");

        let result = assert_vector(&fixture, &[1.25, 2.5], 0.0);

        std::env::remove_var(UPDATE_GOLDEN_ENV);
        result.unwrap();
        assert_eq!(
            fs::read_to_string(&fixture).unwrap(),
            "1.25000000000000000\n2.50000000000000000\n"
        );
        assert_vector(fixture, &[1.25, 2.5], 0.0).unwrap();
    }
}
