# CI/CD pipeline (S0-T3)

The pipeline lives in [`.github/workflows/ci.yml`](../.github/workflows/ci.yml).
It runs on **every pull request**, on pushes to `main`, and on manual dispatch.

## Principle: CI runs the justfile, nothing else

CI never re-implements a check. Every gate shells out to an existing target in
the root [`justfile`](../justfile), so the commands that run in CI are literally
the commands a developer runs locally. To add or change a check you edit the
justfile; CI picks it up with no workflow change. This makes local/CI drift
structurally impossible rather than merely discouraged.

## Gates

All gates run in a single job, **`CI / gates`** (ubuntu-latest).

| Step | just target | Rust | Python |
|------|-------------|------|--------|
| build | `just build` | `cargo build --workspace` | `uv sync && uv build` |
| lint / format / types | `just lint` | `cargo fmt --all --check`<br>`cargo clippy --workspace --all-targets -- -D warnings` | `ruff check .`<br>`ruff format --check .`<br>`mypy` (`strict = true`) |
| unit tests | `just test` | `cargo test --workspace` | `pytest -q` |

Coverage against the ticket's contract:

- **Lint** — clippy with `-D warnings` (Rust), ruff `check` (Python).
- **Format-check** — `cargo fmt --check` (Rust), `ruff format --check` (Python).
  Both are check-only; neither rewrites files. `just fmt` is the in-place
  counterpart and is deliberately *not* run by CI.
- **Typecheck** — mypy in strict mode (`[tool.mypy] strict = true` in
  `py/pyproject.toml`). On the Rust side type checking is inherent to
  `cargo build` / `clippy` / `cargo test`, all of which are gates.
- **Unit tests** — `cargo test --workspace` and `pytest`.

Every gate step carries `if: ${{ !cancelled() }}`, so a failure in one gate does
not skip the rest — a single CI run reports *all* the problems instead of only
the first. A failed step still fails the job.

## How a red gate blocks a PR

Two layers:

1. **The job fails.** Each gate is a plain `run:` step, so a non-zero exit fails
   the step and therefore the `gates` job. The justfile itself uses
   `set shell := ["bash", "-eu", "-o", "pipefail", "-c"]` and `just` aborts a
   recipe on the first failing line, so a failure inside a multi-command recipe
   (e.g. clippy inside `lint-rust`) cannot be swallowed. There is no
   `continue-on-error` anywhere in the workflow.
2. **The check is required.** In GitHub, a failing check only *hard-blocks the
   merge button* when it is a required status check. Configure it once:

   *Settings → Branches → Branch protection rule for `main` → **Require status
   checks to pass before merging** → add **`gates`***
   (equivalently `gh api -X PATCH repos/:owner/:repo/branches/main/protection ...`).
   Also enable *Require branches to be up to date before merging* so a PR is
   re-tested against the current `main`.

   With that rule in place the merge button is disabled while `gates` is red or
   pending.

   > **Repo status as of S0-T3:** this cannot be enabled yet. The repository is
   > **private on a free personal plan**, where GitHub gates both branch
   > protection and rulesets — the API returns
   > `403 Upgrade to GitHub Pro or make this repository public to enable this
   > feature` for `/branches/main/protection` *and* `/rulesets`. Enforcement
   > therefore needs one of: making the repo public, upgrading to Pro/Team, or
   > moving it under an organization. Until then `gates` still runs on every PR
   > and reports a red ❌ that reviewers can see and act on — the gate is
   > advisory rather than mechanically enforced. That is a repository plan
   > constraint, not a pipeline gap: the workflow already fails correctly, and
   > it needs no change when protection is switched on.

The job name is stable (`gates`) precisely so the branch-protection rule does
not need updating when steps are added.

## Golden-test harness (S0-T4)

The golden-test harness ships in **S0-T4** and is wired into `just test`. Because
CI invokes the aggregate `just test` target rather than enumerating individual
test commands, the golden gate becomes a blocking CI gate the moment S0-T4
merges — **no change to this workflow is required**. This workflow intentionally
adds no golden-specific step, since those files do not exist yet.

## Pinned toolchain

CI installs exactly the versions the devcontainer uses
(`.devcontainer/Dockerfile`):

| Tool | Version | Pinned where | How CI gets it |
|------|---------|--------------|----------------|
| Rust | 1.97.1 (+ clippy, rustfmt) | `rust-toolchain.toml` | `rustup show active-toolchain \|\| rustup toolchain install` — reads the file, so the version is **not** duplicated in the workflow |
| uv | 0.11.24 | `.devcontainer/Dockerfile`, workflow | `astral-sh/setup-uv` (pinned to `v9.0.0` — the action publishes no floating major tag) |
| just | 1.58.0 | `.devcontainer/Dockerfile`, workflow | `taiki-e/install-action` |
| Python | 3.12 | `py/.python-version` | provisioned by `uv sync`, so the version is **not** duplicated in the workflow |

Two versions (uv, just) are stated in the workflow because they are installed
before any repo file can declare them; both are commented as needing to stay in
sync with the devcontainer. Bumping Rust or Python requires touching only their
respective pin files.

## Caching

- **cargo** — `Swatinem/rust-cache` scoped to the `rust` workspace: caches the
  registry, git checkouts and `rust/target`, keyed on `Cargo.lock` plus the
  compiler version.
- **uv** — `setup-uv` cache keyed on `py/uv.lock` (`cache-dependency-glob`),
  plus `cache-python: true` to cache the managed CPython download.

Caches are keyed on the lockfiles, so a dependency change invalidates them
automatically and a stale cache can never mask a broken resolve.

## Other workflow settings

- `permissions: contents: read` — least privilege; CI needs no write scopes.
- `concurrency` with `cancel-in-progress` — a new push to a PR cancels the
  superseded run.
- `timeout-minutes: 30` — a hung gate fails rather than occupying a runner.

## Running the gates locally

```bash
just build && just lint && just test
```

That is the whole pipeline. If it is green locally on the pinned toolchain, it
is green in CI.
