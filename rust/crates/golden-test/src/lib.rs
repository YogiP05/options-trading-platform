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
        if error > tolerance {
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
            line.parse::<f64>().map_err(|error| GoldenError {
                fixture: path.to_path_buf(),
                detail: format!("fixture line {} is not a number: {error}", index + 1),
            })
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
