"""Placeholder benchmark harness (S0-T1).

Substrate only: times a trivial call so `just bench` has a real, clean run on
the Python side. Real numerical benchmarks arrive with the pricing core (S2+).
"""

from __future__ import annotations

import time

from t_plat import add


def main() -> None:
    iters = 1_000_000
    start = time.perf_counter()
    total = 0
    for i in range(iters):
        total = add(total, i)
    elapsed = time.perf_counter() - start
    rate = iters / elapsed / 1e6
    print(f"[py bench] add x{iters}: {elapsed * 1e3:.2f} ms ({rate:.2f} M ops/s)")


if __name__ == "__main__":
    main()
