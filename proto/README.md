# `proto/` — Cross-language schemas & interface contracts

The single source of truth for data shapes that cross a language or process
boundary: the normalized tick/quote schema (S1), message-bus payloads
(NATS/JetStream), and any Rust↔Python wire contracts.

Keeping these definitions here — rather than hand-writing structs on each side —
is how we hold to principle **P4** (backtest and live share one implementation):
both languages generate their types from the same schema.

## Status (S0-T1)

Empty placeholder. No schemas exist yet — S0-T1 is build substrate only. The
first real schema (the normalized tick/quote model) lands in **S1**.

When populated, this directory will also document the code-generation step and
wire it into the `just build` target.
