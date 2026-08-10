# UI Design — Operator & Research Console

> **Status:** Design / pre-build. No UI code exists yet.
> **Companion to:** `options-platform-plan.md` (architecture & strategy),
> `build-stage-plan.md` (build stages + cross-cutting tracks),
> `docs/adr/ADR-009-ui-read-side-console.md` (the governing decision).
> **Last updated:** 2026-08-10

This document describes *what* a UI for the platform should be, *where* it
attaches, and *how* to phase it. The single most important rule — the UI lives
on the read/analytics side and never in the execution-critical path — is decided
in **ADR-009**; this doc elaborates it.

---

## 1. Purpose & non-goals

**Purpose.** Give humans a way to (a) **monitor** the live system safely,
(b) **research** vol surfaces, dealer positioning, and backtests, and (c) watch
**shadow-mode telemetry** so the dealer signal can be judged before it ever
touches live hedging (`options-platform-plan.md` §2).

**Non-goals.**
- **Not** a retail trading app or an order-entry terminal for discretionary
  clicking.
- **Not** a microsecond view of the book. This is not an HFT cockpit (P3); the
  UI updates at analytics-clock cadence.
- **Not** a control plane that writes engine state directly. Control is a
  narrow, audited channel through the OMS/safety layer (§6).

## 2. Audiences & primary views

### 2.1 Ops / monitoring ("keep us solvent")
- **Portfolio net greeks** (delta/gamma/vega/theta + vanna/vomma/charm) —
  consolidated, since we hedge net greeks not per-position (§8.5).
- **Positions & fills**, **P&L attribution** (per-strategy virtual books
  reconciled to real fills), hedge activity trace.
- **Risk-limit status**: dollar-gamma limits, band state, event exclusions.
- **A prominent, always-visible kill switch state** (and, later, a guarded
  control — see §6). The kill switch itself remains independent of the UI.
- **Alerts**: limit breaches, stale signals, surface-quality degradation,
  reconciliation mismatches.

### 2.2 Research / analytics
- **Vol surface** visualization (SVI/SSVI) with **arb-free diagnostics** and the
  per-point residual z-scores / local RMSE that Strategy C trades on (§3.C).
- **Dealer positioning**: GEX-by-strike, **flip intervals**, pin scores — all
  uncertainty-first (see §4).
- **Backtest results**: quote-level P&L, cost attribution, hedge traces, run
  manifests (data + code + config versions) so results are reproducible (X1/S5).
- **Signal telemetry**: break-even-variance entry conditions, RV forecasts, band
  widths over time.

### 2.3 Shadow-mode telemetry (the reason a UI earns its keep early)
Per §2, the hedge controller logs, in shadow mode: the baseline hedge
recommendation, the GEX-conditioned recommendation, their **difference**,
counterfactual cost/P&L, model confidence, and signal freshness. A dashboard of
these is what makes the **Phase 4 activation gate** (`J = E[hedged P&L] −
λ₁·Var − λ₂·E[cost] − λ₃·E[tail]`) legible and defensible.

## 3. Where the UI attaches (architecture)

```
        Execution/Risk clock (Rust, sub-ms, local only)  ──►  [ NO UI HERE ]
                       │ (emits events, never blocks on UI)
                       ▼
   NATS/JetStream bus ──┬──────────────► Read-gateway (subscribes to bus)
                        │                        │  WebSocket / SSE
   Analytics clock ─────┤                        ▼
   (surface, GEX,       │                    Browser UI  ◄── BFF read API ──┐
    signals, regime)    │                                                   │
                        ▼                                                    │
   Stores:  ClickHouse (ts greeks/GEX/signals) ─────────────────────────────┤
            Postgres  (positions/orders/fills/P&L, source of truth) ────────┘

   Command channel (separate, authenticated, audited):
       Browser ──► BFF ──► OMS / safety layer ──► (kill switch stays independent)
```

**Key points**
- The **read-gateway** is a dedicated service that subscribes to the bus and
  fans out live updates to browsers over WebSocket/SSE. It is outside the
  execution-critical path; if it dies, trading is unaffected.
- The **BFF (backend-for-frontend) read API** answers historical/paged queries
  against ClickHouse (time-series) and Postgres (positions/P&L).
- The **command channel** is physically separate from reads and routes through
  the OMS/safety layer (§6).

## 4. Uncertainty-first rendering (a hard requirement, not a style choice)

P1: dealer positioning is a *probabilistic latent state, never an oracle*; §8.2:
"never trade a point estimate or a single flip level." The UI must **embody**
this:

- Show **`P(GEX>0 | data)`** and a **flip interval** (e.g. `S* ∈ [5170, 5205]`),
  never a single flip line.
- Always display **model confidence** and **signal freshness** next to any
  positioning figure; de-emphasize/grey stale or low-confidence signals.
- Show the **four-model ensemble** spread (naive-OI / signed-flow / scenario /
  latent-state) as a range, not a consensus number.
- Pinning/squeeze candidates carry their **invalidation condition** in the view.

A UI that renders GEX as a crisp number would actively encourage the exact
mistake the strategy notes warn against. Uncertainty-first is therefore a
correctness requirement for this UI.

## 5. Tech approach (deferred specifics, sound defaults)

- **Phase 1 (cheap):** **Grafana** (or Metabase) directly over ClickHouse +
  Postgres. Gets backtest P&L, greeks history, cost attribution, and
  surface-quality panels with near-zero bespoke code, and exercises the read-side
  path. No frontend build.
- **Phase 2 (bespoke console):** **React + TypeScript** frontend with a charting
  library suited to surfaces/curves; a **BFF** in either **FastAPI (Python)** —
  natural next to the research/analytics stack — or **axum (Rust)** if we want the
  read API close to the engine types. The **read-gateway** relays NATS→browser.
- **Reuse the one implementation:** any pricing/greeks the UI needs comes from
  the same Rust core exposed via PyO3 (X5), never a reimplementation.

Concrete frontend/BFF/gateway-protocol choices are deferred to a follow-up ADR
when a bespoke console is greenlit.

## 6. Control surfaces & safety

Read-only views ship first and freely. **Any control** — kill switch trigger,
enable/disable a strategy, adjust bounded band parameters, or promote/demote the
dealer signal (§2 staged rollout) — must:

1. Go through the **command channel → OMS/safety layer**, never a direct write to
   engine state.
2. Be **authenticated and authorized** (role-based; not every viewer can act).
3. Be **audited** (who/what/when, tied to the Postgres source of truth).
4. Respect that the **independent kill switch remains authoritative below the
   strategy layer** (X2/§5) — the UI can *request* a halt, but the safety layer,
   not the UI, is the authority.

Control features are therefore gated on the authz + audit work, while
monitoring/research dashboards are not.

## 7. Phasing & where it fits the build

The UI is a facet of the **X3 Observability** cross-cutting track, not a stage on
the critical path. Suggested cadence:

| When | UI increment |
|---|---|
| Phase 0 (foundations) | Grafana over Postgres/ClickHouse: backtest P&L, greeks history, surface-quality + cost-attribution panels. Read-only. |
| Phase 1 (delta-neutral core) | Live net-greek + hedge-activity + P&L-attribution monitoring; risk-limit/alert panels. Read-only. |
| Phase 2 (positioning, shadow) | Surface + GEX/flip-interval/pin views (uncertainty-first) and the **shadow-mode telemetry** dashboard (§2). |
| Phase 4 (activation) | `J`-objective dashboards (baseline vs conditioned, out-of-sample); first **guarded control surfaces** with authz + audit. |

Nothing here changes the stage order in `build-stage-plan.md`; the UI grows
alongside the components it observes.

## 8. Non-negotiables (recap)

1. **Never in the execution-critical path** (ADR-009, §8.5).
2. **Uncertainty-first** for all positioning signals (P1, §8.2).
3. **Control routes through OMS/safety with authz + audit**; the kill switch
   stays independent (X2/§5).
4. **One pricing implementation** — reuse the Rust core via PyO3, never a
   UI-side reimplementation (X5).

## 9. Open questions

- Grafana vs. a bespoke console: how long does Grafana carry us before the
  shadow-mode/`J` views justify bespoke work?
- BFF language: FastAPI (closer to research) vs. axum (closer to engine types)?
- Read-gateway transport and back-pressure model (WebSocket vs. SSE; per-topic
  fan-out; snapshot+delta vs. full refresh).
- AuthN/Z provider and the audit schema for the command channel.
- Multi-user concurrency on control actions (locking / last-writer semantics).

---

*End of UI design doc.*
