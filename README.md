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
├── config/        Typed config: schema, profiles, sample.  See config/README.md
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

## Configuration & secrets (S0-T5)

Typed config with one schema and two implementations that must agree — Rust
([`rust/crates/platform-config`](./rust/crates/platform-config)) and Python
([`py/src/t_plat/config`](./py/src/t_plat/config)) — both reading the same TOML
files in [`config/`](./config). Substrate only: connectivity, logging and
telemetry, no trading settings.

**Precedence — `env > file > defaults`** (lowest to highest, each deep-merging
over the one below):

1. Built-in defaults in code (`Config::default()` / `Config.defaults()`)
2. `config/default.toml` — base values for every profile
3. `config/<profile>.toml` — `local` or `prod`, selected by `T_PLAT_PROFILE`
4. `$T_PLAT_CONFIG_FILE` — optional explicit file, must exist if set
5. `T_PLAT__<SECTION>__<KEY>` — typed environment overrides, beat every file

**Local vs prod** is behavioural, not just different values: `local` tolerates
unresolved secrets and allows `app.debug`, while `prod` fails to boot on any
missing credential and rejects `app.debug`.

**The two implementations are held to the same numeric bounds.** Every numeric
field has one canonical range enforced in both languages, and
`config/testdata/numeric_bounds.toml` is a shared fixture whose boundary cases
(min, max, max+1, negative, overflow) both test suites run — so Rust and Python
can never disagree about which configs are valid.

**No secret is committed.** Secret-typed fields hold a *reference* —
`env:NAME`, `file:/path` (a Docker/K8s secret mount), or `none` — that the
loader dereferences at load time. A literal in a config file is a hard load
error, covered by tests in both languages.

Get started:

```bash
cp .env.example .env                                  # git-ignored; fill in locally
cp config/config.example.toml config/local.override.toml
```

Full schema, precedence table and the local-vs-prod contract:
[`config/README.md`](./config/README.md).
