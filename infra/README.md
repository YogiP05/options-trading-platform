# `infra/` — Infrastructure, dev-env & deployment

Reproducible environments and (later) deployment/orchestration config.

## Reproducible dev environment: **devcontainer** (and why)

The dev environment is a [Dev Container](https://containers.dev/) defined at
[`../.devcontainer/`](../.devcontainer/):

- `Dockerfile` — pins the toolchain: **Rust 1.97** (base image), **uv 0.11.24**,
  and **just 1.58.0**.
- `devcontainer.json` — mounts the repo, runs `uv sync` on create, and installs
  the rust-analyzer / ruff / python editor extensions.

We chose a devcontainer over Nix because it is the lowest-friction path that
works identically in VS Code, JetBrains Gateway and CI, using ordinary Docker
that the team already runs. It gives us one pinned image as the single source of
truth for toolchain versions, mirrored by:

- `../rust-toolchain.toml` — pins the Rust toolchain for host (non-container)
  users too.
- `../py/uv.lock` — pins the full Python dependency graph.

### Using it

- **VS Code / JetBrains:** "Reopen in Container" — everything is preinstalled;
  then run `just build && just test`.
- **Plain Docker:** `docker build -t t_plat-dev .devcontainer` then run `just`
  targets inside a container with the repo mounted at `/workspace`.
- **Host (no container):** install the pinned tools yourself — see the root
  `../README.md` "Prerequisites" section.

## Status (S0-T1)

Only the dev-env definition exists. Deployment/orchestration (message bus,
stores, services) arrives in later stages.
