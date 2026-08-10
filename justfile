# Task runner for the options-trading-platform monorepo (S0-T1).
#
# Every target runs BOTH languages (Rust via cargo, Python via uv) so the
# monorepo has one command per concern. Requires `cargo`, `uv` and `just` on
# PATH — see README.md / infra/README.md for the reproducible dev-env setup.

set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

rust_dir := "rust"
py_dir := "py"

# List available targets.
default:
    @just --list

# --- Build ------------------------------------------------------------------

# Build both language substrates.
build: build-rust build-py

build-rust:
    cd {{rust_dir}} && cargo build --workspace

# `uv sync` provisions the venv + dev tools from uv.lock, then we build a wheel.
build-py:
    cd {{py_dir}} && uv sync && uv build

# --- Test -------------------------------------------------------------------

# Run all tests for both languages.
test: test-rust test-py

test-rust:
    cd {{rust_dir}} && cargo test --workspace

test-py:
    cd {{py_dir}} && uv run --group dev pytest -q

# Verify committed numerical fixtures without changing them.
golden:
    cd {{rust_dir}} && cargo test -p golden-test

# Re-record numerical fixtures after reviewing an intentional output change.
golden-update:
    cd {{rust_dir}} && UPDATE_GOLDEN=1 cargo test -p golden-test

# --- Lint -------------------------------------------------------------------

# Lint + format-check both languages. Warnings fail the build.
lint: lint-rust lint-py

lint-rust:
    cd {{rust_dir}} && cargo fmt --all --check
    cd {{rust_dir}} && cargo clippy --workspace --all-targets -- -D warnings

lint-py:
    cd {{py_dir}} && uv run --group dev ruff check .
    cd {{py_dir}} && uv run --group dev ruff format --check .
    cd {{py_dir}} && uv run --group dev mypy

# --- Bench ------------------------------------------------------------------

# Placeholder benchmarks that run cleanly (real numerics arrive in S2+).
bench: bench-rust bench-py

# No `#[bench]` targets yet: this compiles the workspace in bench mode and exits
# clean, proving the bench path works before any pricing math exists.
bench-rust:
    cd {{rust_dir}} && cargo bench --workspace

bench-py:
    cd {{py_dir}} && uv run --group dev python benches/smoke_bench.py

# --- Housekeeping -----------------------------------------------------------

# Format code in place (not run by CI; `lint` only checks).
fmt:
    cd {{rust_dir}} && cargo fmt --all
    cd {{py_dir}} && uv run --group dev ruff format .

clean:
    cd {{rust_dir}} && cargo clean
    rm -rf {{py_dir}}/dist {{py_dir}}/.venv
