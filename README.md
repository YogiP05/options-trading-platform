# Options Trading Platform — Monorepo

A Rust + Python monorepo for an options-trading platform. This is the **S0-T1**
foundation: it establishes the repo layout and the Rust/Python build toolchain.
There is **no trading, pricing, greeks, surface or strategy logic yet** — this
ticket is build substrate only.

See the planning docs for intent:
[`options-platform-plan.md`](./options-platform-plan.md) (architecture, §5 tech
stack) and [`build-stage-plan.md`](./build-stage-plan.md) (Stage **S0**).

## Repository layout

```
.
├── rust/          Rust core — Cargo workspace (hot path). See rust/README.md
│   └── crates/
│       └── platform-core/   placeholder crate (builds/tests/lints/benches)
├── py/            Python side — uv package (research/services). See py/README.md
│   ├── src/t_plat/          placeholder package
│   ├── tests/               pytest smoke tests
│   └── benches/             placeholder benchmark harness
├── proto/         Cross-language schemas & wire contracts. See proto/README.md
├── docs/          Design docs, ADRs, runbooks.          See docs/README.md
├── infra/         Reproducible dev-env & deployment.    See infra/README.md
├── .devcontainer/ Pinned dev toolchain (Rust/uv/just).
├── rust-toolchain.toml   Pinned Rust toolchain (1.97.1)
└── justfile       Task runner: build / test / lint / bench
```

## Toolchain choices

| Concern            | Choice                        | Rationale |
|--------------------|-------------------------------|-----------|
| Rust build         | Cargo workspace (`rust/`)     | Standard multi-crate layout; shared lint policy. |
| Python packaging   | **uv** (`py/`)                | One fast tool for venv + resolve + lockfile + build. See [py/README.md](./py/README.md). |
| Reproducible env   | **devcontainer** (`.devcontainer/`) | One pinned image for editors + CI. See [infra/README.md](./infra/README.md). |
| Task runner        | **just** (`justfile`)         | Simple, discoverable per-language targets. |

## Prerequisites (host, without the devcontainer)

Pinned versions live in `.devcontainer/Dockerfile` and `rust-toolchain.toml`.

- **Rust** via [rustup](https://rustup.rs/) — the pinned toolchain in
  `rust-toolchain.toml` (1.97.1) installs automatically on first `cargo` call.
  Ensure `cargo` is on your `PATH` (`source "$HOME/.cargo/env"`).
- **[uv](https://docs.astral.sh/uv/)** ≥ 0.11 — `curl -LsSf https://astral.sh/uv/install.sh | sh`.
- **[just](https://github.com/casey/just)** ≥ 1.58 — `brew install just` (or `cargo install just`).

Or skip all of that and **"Reopen in Container"** (see [infra/README.md](./infra/README.md)).

## How to run each target

All targets run **both** languages from the repo root:

```bash
just              # list all targets
just build        # cargo build --workspace   +  uv sync && uv build
just test         # cargo test  --workspace   +  uv run pytest
just lint         # rustfmt --check + clippy -D warnings  +  ruff check/format + mypy
just bench        # cargo bench (clean no-op)  +  python placeholder bench
```

Housekeeping: `just fmt` (format in place), `just clean`. Individual sides are
available too (`just build-rust`, `just test-py`, …); run `just --list`.
