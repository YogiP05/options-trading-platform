# `platform-config` — typed configuration & secret sourcing (S0-T5)

The Rust half of the platform's config layer. The Python half lives in
[`py/src/t_plat/config`](../../../py/src/t_plat/config); both read the same TOML
files in [`config/`](../../../config) and produce the same typed structures.
**[`config/README.md`](../../../config/README.md) is the canonical description
of the schema, the precedence chain and the local-vs-prod split** — this file
covers only the Rust API.

Substrate only: connectivity, logging and telemetry. No trading, pricing or
strategy settings.

## Modules

| Module | Responsibility |
|--------|----------------|
| `model.rs`  | The schema (`Config` and its sections), `Profile`, and validation. |
| `merge.rs`  | Deep-merging config layers and applying typed `T_PLAT__*` overrides. |
| `secret.rs` | `SecretRef`, the redacted `Secret`, `SecretSource`, `ResolvedSecrets`. |
| `loader.rs` | `Loader` — runs the precedence chain, then resolves secrets. |
| `error.rs`  | `ConfigError`, one variant per failure mode. |

## Usage

```rust
use platform_config::Loader;

let loaded = Loader::from_os_env().load()?;   // ./config, T_PLAT_PROFILE, T_PLAT__*
println!("{} @ {}", loaded.config.app.name, loaded.config.profile);

if let Some(password) = loaded.secrets.database_password() {
    connect(password.expose());               // explicit, greppable
}
# Ok::<(), platform_config::ConfigError>(())
```

`Loader` takes its environment and its secret source by injection
(`from_env_map`, `secret_source`), so tests never mutate the process
environment:

```rust
use platform_config::{Loader, MapSecretSource, Profile};
use std::collections::BTreeMap;

let loaded = Loader::from_env_map(BTreeMap::new())
    .profile(Profile::Prod)
    .config_dir("config")
    .secret_source(MapSecretSource::new().with_env("T_PLAT_DATABASE_PASSWORD", "dummy"))
    .load();
```

## Design notes

- **Layers merge as TOML, then deserialize once.** A higher layer states only
  the keys it changes; the schema is applied to the merged document, so every
  layer gets the same `deny_unknown_fields` and type checking.
- **Built-in defaults are a complete config.** That is what lets an environment
  override know the declared type of the key it targets — `T_PLAT__DATABASE__PORT`
  parses as an integer, and `T_PLAT__DATABSE__PORT` is a hard error rather than
  a silent no-op.
- **Secrets are references, not values.** `SecretRef` refuses to deserialize a
  literal, so a committed credential fails `cargo test` instead of shipping.
  `Secret` renders as `Secret(<redacted>)` in `Debug` and `Display`; reading it
  requires `.expose()`.
- **The profile is a behavioural switch.** It selects the `SecretPolicy` and the
  profile validation rules, so `prod` fails to boot on a missing credential and
  `local` does not.
- **Numeric bounds are shared, not incidental.** The `MIN_*`/`MAX_*` constants in
  `model.rs` are mirrored in `model.py`, and `config/testdata/numeric_bounds.toml`
  drives the same boundary cases through both implementations. A bound that moves
  on one side only fails the other side's suite.

## Tests

```bash
cd rust && cargo test -p platform-config
```

- `tests/example_config.rs` — loads the committed `config/config.example.toml`
  and the `local`/`prod` profile files against the real schema, and asserts the
  sample documents exactly the schema's keys.
- `tests/precedence.rs` — one test per layer of the precedence chain, plus the
  error paths (unknown keys, wrong types, literal secrets, missing prod secrets).
- `tests/parity_bounds.rs` — the shared numeric-bounds fixture, asserting the
  same accept/reject outcome Python asserts in `py/tests/test_config_bounds_parity.py`.
- Unit tests live next to the code in `src/*.rs`.
