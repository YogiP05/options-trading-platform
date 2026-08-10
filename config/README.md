# `config/` — typed configuration & secret sourcing (S0-T5)

One schema, one precedence chain, two implementations that must agree:

| Language | Module | Docs |
|----------|--------|------|
| Rust   | [`rust/crates/platform-config`](../rust/crates/platform-config) | [crate README](../rust/crates/platform-config/README.md) |
| Python | [`py/src/t_plat/config`](../py/src/t_plat/config)               | [`py/README.md`](../py/README.md) |

Both read **the same TOML files in this directory** and produce the same typed
structs. Substrate only — no trading, pricing or strategy settings live here.

## Files

| File | Committed? | Purpose |
|------|-----------|---------|
| `default.toml`        | yes | Base values for every profile. Non-secret. |
| `local.toml`          | yes | `local` profile overlay (workstation + CI). Non-secret. |
| `prod.toml`           | yes | `prod` profile overlay (deployed). Non-secret; secrets by reference only. |
| `config.example.toml` | yes | **Sample config** — every key with its type. Loaded by the Rust and Python test suites so it cannot drift. |
| `../.env.example`     | yes | **Sample environment** — control vars + the secret env var names. Dummy values only. |
| `*.override.toml`     | **no** (git-ignored) | Your personal overrides. `cp config.example.toml local.override.toml`. |
| `../.env`             | **no** (git-ignored) | Your local secrets. `cp ../.env.example ../.env`. |

## Precedence — `env > file > defaults`

Layers, **lowest to highest**. Each layer deep-merges over the one below it
(tables merge key-by-key; scalars and arrays are replaced wholesale), so a
higher layer only needs to state what it changes.

| # | Layer | Source | Notes |
|---|-------|--------|-------|
| 1 | Built-in defaults | Rust `Config::default()` / Python `Config.defaults()` | The schema always loads, even with no files and no env. |
| 2 | Base file | `<config_dir>/default.toml` | Optional; skipped if absent. |
| 3 | Profile file | `<config_dir>/<profile>.toml` | Optional; `<profile>` is `local` or `prod`. |
| 4 | Explicit file | `$T_PLAT_CONFIG_FILE` | Optional; **must exist** if the variable is set. |
| 5 | Environment | `T_PLAT__<SECTION>__<KEY>` | Beats every file. |

Secrets sit outside this chain: layers 1–5 only ever carry a secret
*reference*; the secret *value* is resolved afterwards from the environment or
a secret store (see below).

### Control variables (single underscore)

These choose *what* to load rather than overriding a value.

| Variable | Default | Meaning |
|----------|---------|---------|
| `T_PLAT_PROFILE`     | `local`    | `local` \| `prod`. Invalid values are an error. |
| `T_PLAT_CONFIG_DIR`  | `./config` | Directory holding `default.toml` and `<profile>.toml`. |
| `T_PLAT_CONFIG_FILE` | unset      | Extra file layered above the profile file. |

The profile is chosen by the environment **only** — a `profile` key inside a
TOML file is rejected as an unknown key, so a config file can never silently
promote itself to `prod`.

### Value overrides (double underscore)

`T_PLAT__<SECTION>__<KEY>` maps to `[<section>] <key>`, lowercased. `__`
separates nesting levels; single underscores stay inside a segment, so
`T_PLAT__MARKET_DATA__TIMEOUT_MS` targets `market_data.timeout_ms`.

Overrides are **typed and checked**, not stringly-typed:

- the value is parsed as the type the schema declares for that key —
  `T_PLAT__DATABASE__PORT=nope` is a load error, not a silent `0`;
- a variable that targets a key the schema does not define is a load error, so
  a typo like `T_PLAT__DATABSE__PORT` fails loudly instead of doing nothing.

Unknown keys inside the TOML files are rejected the same way.

## Secrets — never committed, never literal

No secret value exists anywhere in this repository, and the loader actively
enforces that. Secret-typed fields accept only a **reference**:

| Reference | Resolved from |
|-----------|---------------|
| `"env:NAME"`   | environment variable `NAME` (the deployment platform's secret injection) |
| `"file:/path"` | a secret-store mount — Docker/Compose secrets, Kubernetes secret volumes |
| `"none"`       | no secret configured |

Anything else — a literal value written into the file — fails to load with a
`SecretLiteral` / `SecretLiteralError` naming the offending key. That check is
covered by a test in both languages, so a committed credential breaks the
build rather than shipping.

Resolved values are wrapped in a `Secret` type whose `Debug`/`Display`
(Rust) and `repr`/`str` (Python) render `Secret(<redacted>)`. Reading the
material requires an explicit `.expose()` call, so a secret cannot reach a log
line by accident.

## Local vs prod — what actually differs

The split is behavioural, not just different values. The profile selects a
**secret policy** and a set of **validation rules**, both enforced at load time:

| | `local` | `prod` |
|---|---|---|
| Secret policy | **Permissive** — a reference that does not resolve (missing env var, unreadable mount, `"none"`) yields *unset*. You can boot without provisioning every credential. | **Required** — every declared secret must resolve, or `load()` fails. A missing credential is a boot failure, not a 3am surprise at first use. |
| `app.debug = true` | allowed | **rejected** at load time |
| Typical values | localhost endpoints, generous timeouts, telemetry off | internal hostnames, tight timeouts, telemetry on with 10% sampling |

Everything else about the two paths is identical — same schema, same loader,
same code — so `local` genuinely exercises what `prod` will run.

## Quick start

```bash
cp .env.example .env                                  # then edit; .env is git-ignored
cp config/config.example.toml config/local.override.toml
export T_PLAT_PROFILE=local T_PLAT_CONFIG_FILE=config/local.override.toml
```

```rust
use platform_config::{Loader, Profile};

let loaded = Loader::from_os_env().load()?;           // reads ./config by default
println!("{} @ {}", loaded.config.app.name, loaded.config.profile);
if let Some(pw) = loaded.secrets.database_password() {
    connect(pw.expose());                             // explicit, greppable
}
```

```python
from t_plat.config import load_config

loaded = load_config()                                # reads ./config by default
print(loaded.config.app.name, loaded.config.profile)
pw = loaded.secrets.database_password()
if pw is not None:
    connect(pw.expose())
```

Both loaders accept an injected environment mapping and secret source, which is
how the test suites cover precedence without mutating the real process
environment.

## Adding a key

1. Add it to the Rust struct in `rust/crates/platform-config/src/model.rs` and
   the Python dataclass in `py/src/t_plat/config/model.py` (same name, same type).
2. Add it to `config/default.toml` **and** `config/config.example.toml`.
3. If it is a secret, register it in `secret_refs()` (Rust) / `secret_refs()`
   (Python) so the `prod` fail-fast check covers it, and add the env var name
   to `.env.example`.

The example-config tests in both languages load `config.example.toml` against
the real schema, so a key added to one side and forgotten on the other fails
`just test`.
