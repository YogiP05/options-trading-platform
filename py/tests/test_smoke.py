"""Smoke tests proving the Python package imports and behaves."""

from __future__ import annotations

from t_plat import add, banner


def test_banner_mentions_core() -> None:
    assert "python core" in banner()


def test_add_is_sane() -> None:
    assert add(2, 40) == 42
