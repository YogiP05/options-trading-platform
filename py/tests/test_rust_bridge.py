"""End-to-end test for the native Rust-to-Python bridge."""

from __future__ import annotations

from t_plat.rust_bridge import rust_banner


def test_rust_banner_comes_from_native_extension() -> None:
    assert rust_banner() == "options platform bridge: Rust"
