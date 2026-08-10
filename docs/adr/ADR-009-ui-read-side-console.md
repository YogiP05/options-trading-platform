# ADR-009 — The platform UI is a read-side/command console, never in the hot path

- **Status:** Proposed
- **Date:** 2026-08-10
- **Deciders:** Engineering + quant
- **Related:** `options-platform-plan.md` (§4 System Architecture, §2 Sequencing,
  P1/P3/§8.5), `build-stage-plan.md` (X2 Safety, X3 Observability),
  `docs/ui-design.md` (full design)

---

## Context

The architecture (`options-platform-plan.md` §4) already lists **dashboards**
alongside audit / P&L attribution / alerts / kill switches as outputs of the
system. We need a decision on *how* any user interface attaches to the platform
before we build one, because the platform is organized around **three clocks**
with a hard latency-tier rule:

> **Execution / Risk clock** — event-driven, sub-ms to tens of ms. Local Rust
> state only. **No Python or remote DB in this path.** (§4, §8.5)

A naive UI ("read positions straight from the trading engine", "let the operator
click to hedge") would put a network hop, a database, or a user-driven control action
directly into the loop that must stay fast and local — violating P3/§8.5 and
endangering the cleanest P&L stream.

Two more constraints shape the decision:

- **P1** — dealer positioning / GEX is a *probabilistic latent state, never an
  oracle*. Anything the UI renders about positioning must carry uncertainty.
- **X2 / §5** — the kill switch and risk limits live **below/outside** the
  strategy layer and must remain independently authoritative.

## Decision

**The UI is a read-side consumer plus a narrow, audited command channel. It
attaches only to the Analytics and Reconciliation/Research clocks and the data
stores — never to the Execution/Risk clock.**

Concretely:

1. **Reads** come from the event bus (NATS/JetStream) and the stores —
   **ClickHouse** (time-series greeks/GEX/signals) and **Postgres** (positions/
   orders/fills/P&L, the reconciled source of truth). A dedicated **read-gateway**
   service subscribes to the bus and relays to the browser over WebSocket/SSE. The
   read-gateway is outside the execution-critical path.
2. **The execution/risk hot path never serves the UI directly** and never takes a
   dependency on it. If the UI, the read-gateway, ClickHouse, or the browser is
   slow or down, hedging and kill switches are unaffected.
3. **Control actions** (kill switch, enable/disable a strategy, adjust bounded
   band parameters, promote/demote the dealer signal per §2) are a **separate,
   authenticated, audited command channel** into the OMS/safety layer — not
   direct writes from the UI to engine state. The independent kill switch remains
   authoritative below the strategy layer.
4. **Positioning is rendered uncertainty-first** (P1): `P(GEX>0)`, flip
   *intervals*, pin scores with confidence and signal freshness — never a single
   deterministic GEX number or flip line.
5. **Phasing is Grafana-first.** Early dashboards are Grafana/Metabase over
   ClickHouse + Postgres (no bespoke frontend); a bespoke operator/research
   console comes later, when there is a live system and shadow-mode telemetry
   (§2) worth visualizing.

## Consequences

**Positive**
- The latency tier (§8.5) is structurally protected: the UI cannot stall or
  corrupt the execution loop because it is never in it.
- Cheap early value: Grafana on the existing stores yields backtest/greeks/P&L
  dashboards with near-zero bespoke code, and exercises the read-side path.
- The read/command split keeps the audit trail and authorization boundary clean;
  the kill switch stays independent (X2).
- Uncertainty-first rendering makes the UI reinforce P1/§8.2 instead of
  encouraging the "GEX-as-truth" mistake.

**Negative / costs**
- A read-gateway (bus→browser relay) and a BFF/read API are additional services
  to build and operate.
- Live data reaches the UI at analytics-clock cadence (100 ms–minutes), not
  hot-path speed. That is correct for monitoring/research but means the UI is not
  a microsecond view of the book (which is fine — this is not an HFT cockpit).
- The command channel needs real authz + audit before any control surface ships;
  that work gates the "operator can act" features (not the read-only ones).

**Neutral**
- Tech choices (Grafana first; later React/TypeScript frontend + a FastAPI or
  axum BFF) are deferred to `docs/ui-design.md` and a later ADR if a bespoke
  console is greenlit.

## Alternatives considered

- **Embed a UI/state read directly in the trading engine.** Rejected — puts a
  DB/network/user-facing dependency in the execution-critical path (P3/§8.5).
- **Skip a UI, use logs + notebooks only.** Viable for the earliest phases, but
  shadow-mode telemetry (§2) and the Phase 4 activation gate (`J`) are far more
  legible with dashboards; Grafana-first captures most of that value cheaply.
- **Let the UI write engine state directly for control.** Rejected — control
  must route through the OMS/safety layer with authz + audit; the kill switch
  must remain independently authoritative (X2/§5).

## Follow-ups

- If/when a bespoke console is greenlit, write a follow-up ADR pinning the
  frontend/BFF stack and the read-gateway protocol.
- Define the command-channel authz + audit model before any control surface.
