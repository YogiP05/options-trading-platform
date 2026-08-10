# `py/` — Python side (research, services, tooling)

Home for the platform's **research / backtest / analytics** code (tech-stack
plan §5). S0-T1 ships only a placeholder package (`t_plat`) so the Python build
substrate is provably wired up — no domain logic yet.

## Packaging tool: **uv** (and why)

We use [uv](https://docs.astral.sh/uv/) rather than Poetry:

- **One tool, fewer moving parts** — uv covers virtualenv creation, dependency
  resolution, a committed lockfile (`uv.lock`), running commands, and PEP 517
  builds. Poetry needs extra pieces for equivalent speed/reproducibility.
- **Reproducible** — `uv.lock` pins the full resolved graph; `uv sync` recreates
  the exact environment in the devcontainer and CI.
- **Fast** — resolutions/installs are dramatically quicker, which keeps the
  `just` targets snappy.
- **Standards-first** — a plain PEP 621 `pyproject.toml` with the `hatchling`
  build backend, so we are not locked into a tool-specific manifest.

## Layout

- `pyproject.toml` — PEP 621 metadata, dev dependency group, ruff + mypy config.
- `.python-version` — pins the interpreter (3.12) uv provisions; satisfies
  `requires-python` (>=3.11) and matches the devcontainer.
- `uv.lock` — committed lockfile (the reproducible resolution).
- `src/t_plat/` — the placeholder package (src-layout).
- `tests/` — pytest smoke tests.
- `benches/` — placeholder benchmark harness for `just bench`.

## Running

From the repo root, prefer the `just` targets:

```bash
just build   # uv sync + uv build (wheel/sdist)
just test    # uv run pytest
just lint    # ruff check + ruff format --check + mypy
just bench   # placeholder timing script (clean run)
```

Or directly, build the native extension before pytest:

```bash
cd py
uv sync
uv run --group dev maturin develop -F extension-module --manifest-path ../rust/crates/platform-py/Cargo.toml
uv run --group dev pytest -q
```

The standard `just test` target runs `maturin develop` before pytest. Maturin
builds the `platform-py` PyO3 crate and installs its native extension into uv's
managed environment, so bridge tests always exercise freshly compiled Rust.
