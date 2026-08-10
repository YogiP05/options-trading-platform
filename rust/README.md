# `rust/` — Rust core (Cargo workspace)

Home for the platform's **hot-path** code: feed state, incremental greeks, IV
solvers, SVI/SSVI surface, OMS and hedge control (per the tech-stack plan §5).
Rust is chosen for memory safety on order-sending components and numerical
performance.

## Layout

- `Cargo.toml` — the workspace manifest (`members = ["crates/*"]`, shared lint
  policy, shared package metadata).
- `crates/platform-core/` — placeholder foundation crate for S0-T1. Builds,
  tests, lints and benches so the workspace is provably wired up. **No domain
  logic yet** — that arrives in later stages.
- `crates/platform-config/` — typed configuration loading and secret sourcing
  (S0-T5). Reads `../config/`; see [`crates/platform-config/README.md`](./crates/platform-config/README.md)
  and the shared schema in [`../config/README.md`](../config/README.md).

New crates are added under `crates/<name>/` and are picked up automatically by
the `crates/*` glob.

## Running

From the repo root, prefer the `just` targets (they run Rust + Python together):

```bash
just build   # cargo build --workspace
just test    # cargo test  --workspace
just lint    # cargo fmt --check + cargo clippy -D warnings
just bench   # cargo bench --workspace (no bench targets yet → clean exit)
```

Or directly: `cd rust && cargo test --workspace`.

The toolchain is pinned by `../rust-toolchain.toml` (Rust 1.97.1).
