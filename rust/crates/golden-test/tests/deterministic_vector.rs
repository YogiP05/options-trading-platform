//! End-to-end demonstration of an approved numerical golden fixture.

use std::path::PathBuf;

use golden_test::assert_vector;

const TOLERANCE: f64 = 1.0e-12;

fn deterministic_numerical_output() -> Vec<f64> {
    (1..=5)
        .map(|index| {
            let input = f64::from(index) / 2.0;
            input * input + 0.125
        })
        .collect()
}

#[test]
fn deterministic_vector_matches_approved_golden() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("deterministic_vector.golden");

    assert_vector(fixture, &deterministic_numerical_output(), TOLERANCE)
        .unwrap_or_else(|error| panic!("{error}"));
}
