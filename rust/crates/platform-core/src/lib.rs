//! Build-substrate placeholder crate for the options trading platform.
//!
//! S0-T1 ships the monorepo build toolchain only. This crate exists so the
//! Cargo workspace has something real to build, test, lint and bench. It holds
//! **no** trading, pricing, greeks, surface or strategy logic — that is
//! explicitly out of scope until later stages (see the S0-T1 anti-goals).

/// Return the substrate banner for the Rust core.
///
/// Handy as a liveness check that the workspace compiles and links.
#[must_use]
pub fn banner() -> String {
    format!("t_plat rust core v{}", env!("CARGO_PKG_VERSION"))
}

/// Add two integers.
///
/// A deliberately trivial pure function so `cargo test` and `cargo bench` have
/// something concrete to exercise before any domain code exists.
#[must_use]
pub fn add(a: i64, b: i64) -> i64 {
    a + b
}

#[cfg(test)]
mod tests {
    use super::{add, banner};

    #[test]
    fn banner_mentions_core() {
        assert!(banner().contains("rust core"));
    }

    #[test]
    fn add_is_sane() {
        assert_eq!(add(2, 40), 42);
    }
}
