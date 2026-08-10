# Pinned, reproducible dev toolchain for the options trading platform (S0-T1).
#
# One image gives every contributor (and CI) the exact same Rust + Python +
# task-runner versions. Toolchain versions are pinned here and mirrored by
# /rust-toolchain.toml (Rust) and py/uv.lock (Python deps).

# Rust base image pins the compiler; keep in sync with /rust-toolchain.toml.
FROM rust:1.97-bookworm

ARG UV_VERSION=0.11.24
ARG JUST_VERSION=1.58.0
# Pinned Python for a deterministic env. Must satisfy py/pyproject.toml's
# requires-python (>=3.11) and match py/.python-version.
ARG PYTHON_VERSION=3.12

# Clippy + rustfmt for the `just lint` target.
RUN rustup component add clippy rustfmt

# Pinned `uv` (Python packaging tool), copied from its official image.
COPY --from=ghcr.io/astral-sh/uv:0.11.24 /uv /uvx /usr/local/bin/

# Provision the pinned CPython up front so the image is deterministic and uv
# never has to pick/download a different compatible version at runtime.
ENV UV_PYTHON=${PYTHON_VERSION}
RUN uv python install ${PYTHON_VERSION}
RUN ln -sf "$(uv python find ${PYTHON_VERSION})" /usr/local/bin/python3
ENV PYO3_PYTHON=/usr/local/bin/python3

# Pinned `just` task runner.
RUN cargo install just --version ${JUST_VERSION} --locked

WORKDIR /workspace
