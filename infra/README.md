# `infra/` — Infrastructure, dev-env & deployment

Reproducible environments and (later) deployment/orchestration config.

## Reproducible dev environment: **devcontainer** (and why)

The dev environment is a [Dev Container](https://containers.dev/) defined at
[`../.devcontainer/`](../.devcontainer/):

- `Dockerfile` — pins the toolchain: **Rust 1.97** (base image), **uv 0.11.24**,
  **just 1.58.0**, and **Python 3.12** (provisioned via `uv python install`).
- `devcontainer.json` — mounts the repo, runs `uv sync` on create, and installs
  the rust-analyzer / ruff / python editor extensions.

We chose a devcontainer over Nix because it is the lowest-friction path that
works identically in VS Code, JetBrains Gateway and CI, using ordinary Docker
that the team already runs. It gives us one pinned image as the single source of
truth for toolchain versions, mirrored by:

- `../rust-toolchain.toml` — pins the Rust toolchain for host (non-container)
  users too.
- `../py/.python-version` — pins the Python interpreter (3.12) that uv uses.
- `../py/uv.lock` — pins the full Python dependency graph.

### Using it

- **VS Code / JetBrains:** "Reopen in Container" — everything is preinstalled;
  then run `just build && just test`.
- **Plain Docker:** build the image, then start a container with the repo
  bind-mounted as the `/workspace` workspace and run the `just` targets inside:

  ```bash
  # Build the pinned toolchain image (run from the repo root).
  docker build -t t_plat-dev .devcontainer

  # Start an interactive shell with the repo mounted at /workspace.
  docker run --rm -it -v "$PWD":/workspace -w /workspace t_plat-dev bash

  # ...then, inside the container:
  just build && just test && just lint && just bench
  ```

  Or run a target directly without an interactive shell:

  ```bash
  docker run --rm -v "$PWD":/workspace -w /workspace t_plat-dev just test
  ```
- **Host (no container):** install the pinned tools yourself — see the root
  `../README.md` "Prerequisites" section.

## Status (S0-T1)

Only the dev-env definition exists. Deployment/orchestration (message bus,
stores, services) arrives in later stages.
