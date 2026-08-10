"""Python build-substrate package for the options trading platform.

S0-T1 ships the monorepo build toolchain only. This package exists so the
Python side has something real to import, test, lint and bench. It holds **no**
trading, pricing, greeks, surface or strategy logic — that is explicitly out of
scope until later stages (see the S0-T1 anti-goals).
"""

from __future__ import annotations

__version__ = "0.0.0"


def banner() -> str:
    """Return the substrate banner for the Python core."""
    return f"t_plat python core v{__version__}"


def add(a: int, b: int) -> int:
    """Add two integers.

    A deliberately trivial pure function so ``pytest`` and the placeholder
    benchmark have something concrete to exercise before any domain code exists.
    """
    return a + b
