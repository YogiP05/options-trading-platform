# Options Trading Platform — Architecture & Strategy Plan

> **Purpose:** Design blueprint for an options trading platform focused on (1)
> delta-neutral hedging and (2) modeling how market makers suppress and
> accelerate price movements (gamma exposure / dealer positioning, pinning,
> gamma squeezes, vanna/charm flows).
>
> **Provenance:** Synthesized from a structured debate between two independent
> models ("Claude" and "GPT"). Both converged on the blueprint below; residual
> disagreements are flagged explicitly in [§7](#7-open-questions--residual-disagreements).
>
> **Status:** Design / pre-build. No code exists yet.
> **Audience:** Engineering + quant agents implementing the platform.
> **Last updated:** 2026-08-09

---

## Table of Contents

1. [Guiding Principles](#1-guiding-principles)
2. [Core Design Decision: Sequencing](#2-core-design-decision-sequencing)
3. [Trading Strategies](#3-trading-strategies)
4. [System Architecture](#4-system-architecture)
5. [Frameworks & Tech Stack](#5-frameworks--tech-stack)
6. [Phased Build Order](#6-phased-build-order)
7. [Open Questions & Residual Disagreements](#7-open-questions--residual-disagreements)
8. [Developer Notes: What You MUST Understand About the Strategies](#8-developer-notes--what-you-must-understand-about-the-strategies)

---

## 1. Guiding Principles

These are the non-negotiable priors that shape every decision downstream.

- **P1 — Dealer positioning is a probabilistic latent state, not observable truth.**
  Public feeds show trades, quotes, volume, and *lagged* open interest (OI) —
  NOT dealer identity, opening/closing status, or full inventory. "GEX" (Gamma
  Exposure) is a *model*. Its assumptions, confidence, and historical revisions
  must be first-class, versioned data — never presented as an oracle.

- **P2 — A clean, arbitrage-free volatility/risk foundation must come first.**
  The dealer-positioning engine depends on vanna and charm, which are
  derivatives of the vol surface. If the surface is not smooth and arbitrage-free,
  those second-order greeks are numerical noise. **The dealer engine is worthless
  without the arb-free surface underneath it.**

- **P3 — The edge is modeling, not speed.** This is not an HFT system. Do not
  build kernel-bypass networking or chase microseconds. Spend engineering effort
  on correctness, surface quality, execution realism, and cost modeling.

- **P4 — Backtest realism is make-or-break.** The backtester must be
  event-driven, **share the exact production pricing/surface/signal/hedge code**,
  and operate at **quote level**. A gamma-scalp strategy that ignores the bid/ask
  spread looks profitable in backtest and loses money live.

- **P5 — Couple the dealer signal to hedging structurally on day one, but
  activate it economically only after out-of-sample proof.** (See [§2](#2-core-design-decision-sequencing).)

---

## 2. Core Design Decision: Sequencing

**The central architectural resolution** (both models converged here from
opposite starting points):

> **Integrate structurally from day one; activate economically later.**

| Phase | What happens | Gate |
|---|---|---|
| **Structural integration (day one)** | The hedge controller accepts an optional `dealer_regime` context. It runs in **shadow mode**: log the baseline hedge recommendation, the GEX-conditioned recommendation, their difference, counterfactual cost/P&L, model confidence, and signal freshness. | None — build the interface and telemetry immediately. |
| **Economic activation (later)** | Promote the dealer signal to *live* hedge control (modulating band width, urgency, gamma selection). | Must improve an explicit objective **out-of-sample** (see below). |

**Activation objective function:**

```
J = E[hedged P&L] − λ₁·Var(P&L) − λ₂·E[cost] − λ₃·E[tail loss]
```

**Staged rollout for activation:**
1. Baseline cost-aware hedger (no dealer signal).
2. Shadow GEX-conditioned hedger (logs only, no live effect).
3. Small-capital randomized / matched comparison.
4. Bounded modulation (e.g. ±10–20% band width).
5. Larger control authority only after stable, out-of-sample evidence.

**Why this matters:** Gamma scalping has a well-founded statistical basis
(break-even variance). GEX-regime estimation rests on an *assumed dealer sign*.
Hard-wiring a noisy regime signal into the hedger injects that noise into your
cleanest P&L stream. Keep the loops separable and independently measurable.

---

## 3. Trading Strategies

Each strategy is specified as **Signal → Hedge/Execution Rule → Best Conditions
→ Failure Modes**. Delta-neutral core strategies (A–D) are the foundation;
dealer-positioning strategies (E–J) layer on top and are gated per [§2](#2-core-design-decision-sequencing).

### A. Long Gamma Scalping *(the workhorse)*
- **Signal:** Enter when **forecast realized variance exceeds the option's
  executable break-even variance after all frictions** (premium spread, hedge
  spread + impact, expected IV change, overnight/event gaps, skew, discrete
  hedge policy). **Do NOT** simply compare realized vol to displayed ATM IV.
- **Position:** Buy ATM/near-ATM straddle (long gamma, long vega, short theta);
  delta-hedge the underlying.
- **Hedge rule:** **State-dependent no-trade band**, not a pure delta band:
  `bₜ = f(Γ, spread, depth, σ̂_forecast, jump risk, time, inventory, regime)`.
  Widen for cost/impact; narrow for gamma/liquidity/expected short-horizon vol.
  Keep **fixed-time checks as watchdogs** (a pure band can sit unhedged through
  a discontinuity).
- **P&L (approx):** `dP ≈ Θ·dt + ½·Γ·(dS)² + Vega·dσ − hedging costs`
- **Failure modes:** Theta bleed, IV crush, over-hedging microstructure noise,
  overnight jumps, long-option fills above theoretical mid.

### B. Short Gamma / Variance-Risk-Premium Harvesting
- **Signal:** Sell when implied variance materially exceeds your full forecast
  distribution of realized variance **and** jumps, after reserving for tail risk.
- **Position:** Sell straddle/strangle/iron structure with **defined-loss wings**.
- **Risk controls:** Max dollar-gamma limits; **stress with surface shocks
  (skew steepening, term twists) — NOT parallel IV bumps**; hard event exclusions
  or reduced sizing; hedge escalation near concentrated short strikes.
- **Discipline:** The risk unit is **scenario loss, not premium collected.**
  Never describe this internally as "income."
- **Failure modes:** Catastrophic left tail; skew/term-structure moves; gap risk.

### C. Relative-Value Straddles / Strangles
- **Signal:** Surface residual z-score off an **arbitrage-aware SVI/SSVI fit**:
  `z_{K,T} = (σ_market − σ_model) / RMSE_local_regime`. Trade only deviations
  exceeding costs + model uncertainty.
- **Position:** Long cheap tenor / short rich tenor; skew flies; event vs.
  non-event calendars; ETF vs. futures options.
- **Key caveat:** **"Vega neutral is NOT risk neutral."** Legs differ in gamma,
  vanna, charm, liquidity, and jump exposure. Neutralize the risk matching the
  thesis, not delta alone.

### D. Dispersion
- **Signal / thesis:** Explicitly a **correlation trade**. Short index variance
  + long weighted constituent variance = short implied correlation (carry that
  blows up when correlation → 1 in selloffs).
- **Implementation:** Start with top 20–50 constituents (not the full index);
  constrained optimizer to minimize residual delta/vega/sector/earnings/liquidity;
  index futures for index delta, shares for constituent delta.
- **Staging:** Build **after** the single-underlying engine (adds many surfaces,
  earnings gaps, corporate actions, rebalances, high transaction counts).

### E. Gamma-Regime Filter *(conditions everything else)*
- **Signal:** Aggregate dealer gamma over a spot grid:
  `GEX(S) = Σ_j q_j · OI_j · M_j · Γ_j(S,σ_j,T_j) · S² · 0.01`
  where `q_j` = assumed dealer sign, `M_j` = contract multiplier.
- **Estimate via a FOUR-MODEL ENSEMBLE** (each confidence-tagged):
  1. Naive OI model (fixed call/put dealer-sign convention).
  2. Signed-flow inventory model (trade-direction inference; **mandatory
     multi-leg detection**).
  3. Scenario ensemble (GEX under multiple sign assignments; report a range).
  4. Calibrated latent-state model (state-space filter — see below).
- **Trading hypothesis (empirically learned, NOT hard-coded):**
  - Dealers **long gamma** → hedging sells rallies / buys dips → *dampens* moves
    → favor mean-reversion, short realized vol, wider confidence before breakouts.
  - Dealers **short gamma** → hedging buys rallies / sells dips → *amplifies*
    moves → favor breakout/momentum or long convexity.
- **Hard rule:** Do **not** trade merely because aggregate GEX crosses zero.
  Publish `P(GEX(S) > 0 | data)` and a **flip interval**, not a deterministic line.

### F. Gamma-Flip Breakout
- **Signal:** Estimate `S*` where `GEX(S*) = 0`; require **persistence beyond the
  level + abnormal signed volume + rising short-horizon realized vol**.
- **Position:** Directional call/put spread, or straddle managed directionally,
  or underlying with option-defined stop.
- **Discipline:** Publish an interval (e.g. `S* ∈ [5170, 5205]`), never a single
  price. A single tick through a computed flip is not a signal.

### G. Strike Pinning / Expiry Magnet
- **Signal:** Concentration score `P_k = OI_k · Γ_k · e^(−α|S−k|) · L_k`
  (`L_k` = liquidity / executable hedge capacity).
- **Position:** When dealers estimated long gamma, spot near a dominant strike,
  RV declining, no catalyst → bounded mean reversion toward the strike, via
  tightly risk-limited structures.
- **Failure mode:** Pinning is **conditional, not a physical law.** Positioning
  can roll, disappear, or be overwhelmed by directional demand. Exit if the
  strike becomes a launch point rather than a magnet.

### H. Vanna Flow
- **Signal:** `Vanna = ∂Δ/∂σ`. Estimate hedge demand from an expected IV move:
  `ΔQ_hedge ≈ −DealerVanna · Δσ`. Model the **whole surface move** (a "5-vol
  decline" is not parallel across strikes/maturities).
- **Best conditions:** Forecastable IV decline after an event/vol shock.

### I. Charm Flow
- **Signal:** `Charm = ∂Δ/∂t`. Projected hedge demand: `ΔQ_hedge ≈ −DealerCharm · Δt`.
- **Best conditions:** Late session, pre-weekend/holiday, near large expirations,
  0DTE/1DTE inventory. Build **scheduled forecasts** at open/midday/final-hour/
  overnight boundary.

### J. Squeeze Candidate Detection *(single-name)*
- **Signal (rank on):** Estimated negative dealer gamma + customer-initiated call
  buying + near-dated call concentration above spot + short interest/borrow
  utilization + thin book depth + rising IV with positive price/IV co-movement +
  high share-equivalent hedge demand relative to ADV.
- **Position:** Express via **call spreads** or small long-gamma positions.
- **Caveat:** Raw call volume alone is insufficient (could be closing flow,
  spreads, covered calls, or dealer-to-dealer).

---

## 4. System Architecture

**Organizing principle: THREE CLOCKS.** The live hedge controller must never
block on a surface refit, GEX aggregation, or statistical model.

| Loop | Cadence | Responsibility |
|---|---|---|
| **Execution / Risk** | Event-driven, sub-ms to tens of ms | Positions, greeks, pre-trade risk, delta hedges, kill switches. Local Rust state only — **no Python or remote DB in this path.** |
| **Analytics** | 100 ms to minutes | Vol surface, flow classification, GEX, signals, regime state. |
| **Reconciliation / Research** | Intraday to daily | OI reconciliation, model fitting, attribution, retraining. |

### Data Flow

```
OPRA/options feeds ┐
Underlying feeds  ─┼─> Feed handlers ─> Normalized event bus
Reference data    ─┤        │                    │
Rates/dividends   ─┘        └─ raw archive       ├─> Book/quote state
                                                  ├─> Vol surface + Greeks (arb-free SVI/SSVI)
Trade/order events ──────────────────────────────┼─> Flow classifier (trade-direction inference)
                                                  └─> Positioning / GEX state (4-model ensemble)
                                                           │
                     Historical feature store <──── Feature snapshots
                                                           │
                 Research/backtest <── SAME signal code ──> Signal engine
                                                           │
                                          Portfolio optimizer / risk gate
                                                           │
                                             OMS + execution algorithms
                                                           │
                                        Broker/exchange + hedge engine
                                                           │
                        Audit / P&L attribution / alerts / dashboards / kill switches
```

### Component Notes

- **Ingestion:** Do NOT take raw OPRA (firehose, tens of Gbps) unless truly HFT.
  Use a normalized vendor feed (Polygon / Databento / dxFeed / CBOE). Normalize
  everything to one internal tick schema at the edge.
- **Intraday OI:** Official OI is EOD (OCC). Maintain an **intraday OI estimate**
  — but note this is a **latent inventory posterior**, not "yesterday's OI + buys
  − sells." It must estimate customer-vs-dealer side, opening-vs-closing,
  single-leg-vs-complex, paired prints, and exercise/assignment/corrections.
- **Greeks/risk engine:** Stateless service; reprice the book in <1 ms for a few
  thousand contracts on an underlying tick. Compute second-order greeks (vanna,
  vomma, charm, speed).
- **Dealer/GEX engine:** Consumes OI + surface + spot → gamma-by-strike, flip
  interval, net dealer gamma/vanna/charm notional, pin candidates. Runs on the
  analytics clock. Calibration layer runs offline/EOD and feeds parameters back.
- **Latent inventory state-space model** (for the calibrated-latent GEX model):
  ```
  Iₜ = Iₜ₋₁ + Fₜ + εₜ           (I = latent dealer inventory, F = classified flow)
  Yₜ = h(Iₜ, Sₜ, σₜ, Lₜ) + ηₜ   (Y = observed price/vol/hedge-flow footprints)
  ```
  Use next-day OI changes as delayed observations; realized market behavior as
  noisy contemporaneous evidence. **Footprints validate/weight the models — they
  do NOT assign the sign** (that would be circular).
- **Backtester:** Event-driven, shares production pricing/surface/signal/hedge
  code (only the clock, feed, broker, and fill model change), **quote-level**.
- **0DTE module (first-class):** OI resets daily → the intraday-OI estimator IS
  the whole game; gamma is huge but intraday-transient; charm runs on an hourly
  clock.
- **Data stores:** ClickHouse (or kdb+) for time-series greeks/GEX/signals;
  Parquet on S3 (partitioned by date/underlying) for the tick archive;
  Postgres for positions/orders/fills/config/P&L (the reconciled source of truth).

---

## 5. Frameworks & Tech Stack

| Layer | Choice | Rationale |
|---|---|---|
| **Research / backtest / analytics** | **Python** | Decisive ecosystem for research, calibration, dashboards. |
| **Hot path** (feed state, incremental greeks, IV solvers, SVI/SSVI, OMS, hedge control) | **Rust** | Memory safety on order-sending components; numerical perf. |
| **Language bridge** | **PyO3** | Expose the *same* Rust pricing/risk implementation to Python → kills research/live drift. |
| **Pricing reference oracle** | **QuantLib** (C++) — **oracle only** | Use for calendars, curves, American exercise, trees/PDEs, and regression tests. **Do NOT call it in the latency-critical path** (avoids a second pricing implementation diverging from Rust). |
| **IV solver** | Jäckel "Let's Be Rational" (own it in Rust) | Fast, accurate implied-vol inversion. |
| **DataFrames** | Polars | Performance over pandas for large tick data. |
| **Message bus** | NATS / JetStream (over Kafka) | Lower-latency, simpler ops for this scale. |
| **Time-series store** | ClickHouse (or kdb+ if budget/latency demand) | Greeks/GEX/signal history. |
| **Tick archive** | Parquet on S3 | Cheap columnar storage for backtests. |
| **Relational store** | Postgres | Positions/orders/P&L source of truth. |
| **Historical data vendor** | Databento / Polygon | **Quote-level** history (required for realistic backtests). |
| **Backtest/live unification** | Evaluate `nautilus_trader` | Unified engine to eliminate backtest-vs-live divergence. |
| **Broker/execution** | Interactive Brokers → FIX at scale | Start pragmatic, graduate to FIX. |
| **Safety** | Independent execution-layer kill-switch + risk limits | Must operate below/outside the strategy layer. |

---

## 6. Phased Build Order

> Derived from Principles P2–P5. Ship foundations before signals; prove coupling last.

1. **Phase 0 — Foundation.** Arb-free SVI/SSVI surface → smooth greeks (incl.
   vanna/charm); quote-level event-driven backtester sharing live code;
   realistic transaction-cost model; data ingestion + normalized schema; the
   three-clock skeleton with local Rust risk state.
2. **Phase 1 — Delta-neutral core.** Long gamma scalping (break-even-variance
   entry, state-dependent bands); portfolio-level net-greek hedging; defined-loss
   short-vol with surface-shock stress.
3. **Phase 2 — Positioning analytics (shadow).** Four-model GEX ensemble + flow
   classifier + latent-inventory state-space model; flip intervals + pin scores;
   `dealer_regime` logged in **shadow mode** only.
4. **Phase 3 — 0DTE module.** Intraday-OI estimator, hourly charm clock.
5. **Phase 4 — Economic activation.** Promote dealer signal to bounded live hedge
   modulation only after out-of-sample improvement in `J` (staged rollout per §2).
6. **Phase 5 — Advanced strategies.** Dispersion, relative-value vol, single-name
   squeeze detection.

---

## 7. Open Questions & Residual Disagreements

The debate converged strongly, but these remain live:

- **Dealer-sign identification rigor.** Both agree on an ensemble + latent state.
  One view holds the estimator must be a **formal state-space posterior** and
  warns that footprint-based calibration is *circular* if used to *assign* sign
  (low RV → option selling → high GEX estimate can reverse causality). The other
  agrees footprints should only *validate/weight*, not assign. **Resolution in
  practice:** build the state-space estimator; use footprints strictly as a
  validation/emission target.
- **How much hedge authority the dealer signal ever earns.** Gated behind `J`
  improvement; the ceiling is an empirical question, not a design constant.
- **0DTE effect magnitude.** External research (Cboe) cautions that simple
  dealer-gamma / 0DTE narratives may *overstate* the aggregate effect. Treat the
  suppression/acceleration mechanism as a **testable prior**, not a given.

---

## 8. Developer Notes — What You MUST Understand About the Strategies

> Read this section before implementing or trading anything above. These are the
> conceptual traps that make the difference between a system that looks profitable
> in backtest and one that survives live.

### 8.1 Where the money actually comes from
- **Delta-neutral gamma scalping is a bet that *realized* volatility will exceed
  the *implied* volatility you paid — minus all frictions.** You are not
  predicting direction; you are harvesting the gap between realized and implied
  vol by mechanically buying low / selling high as you re-hedge delta.
- **The correct entry test is forecast RV vs. the *executable break-even
  variance after frictions* — NOT "realized vol > displayed ATM IV."** A
  delta-hedged straddle does not earn `RV² − IV²`; the P&L is gamma-weighted,
  path-dependent, strike-dependent, and shifted by discrete hedging and spread
  crossing. If you code the folk version, you will systematically overestimate
  edge.
- **Short volatility is NOT income.** It has attractive average returns and a
  catastrophic left tail. Size it by *scenario loss*, always use defined-loss
  structures, and stress it with **surface shocks (skew/term twists), not
  parallel IV bumps** — that is what actually kills short-vol books.

### 8.2 The dealer/GEX signal is a MODEL, treat it like one
- **You cannot observe dealer positioning.** Every GEX number rests on an
  *assumed* dealer sign. The popular "customers always buy, dealers are always
  short" heuristic is wrong often enough to hurt.
- **Never trade a point estimate or a single flip level.** Publish probabilities
  and intervals. A single tick through a computed gamma flip is noise, not a
  signal — require persistence, abnormal signed volume, and rising realized vol.
- **"Above flip = mean-revert, below flip = trend" is a hypothesis, not a law.**
  It depends on inventory sign, strike/expiry concentration, distance to strikes,
  hedge instrument, customer flow, and liquidity. Learn the relationship
  empirically; do not hard-code it.
- **Pinning and squeezes are conditional.** Positioning can roll, vanish, or be
  overwhelmed by directional demand. Every dealer-flow trade needs a clear
  invalidation condition.

### 8.3 Why the foundation gates everything
- **Your vanna and charm signals are derivatives of the vol surface.** If the
  surface is not arbitrage-free and smooth, your second-order greeks are
  numerical garbage and every dealer-flow strategy built on them is trading
  noise. **Build the arb-free SVI/SSVI surface first — it is a prerequisite, not
  a nicety.**

### 8.4 Costs and execution will decide your fate
- **Transaction costs determine whether a gross edge survives** (though they are
  not the *entire* P&L — vol forecast error, IV repricing, jumps, and skew moves
  all matter too). Model the spread you would actually cross.
- **Backtest at quote level or don't bother.** A bar-based gamma-scalping backtest
  is a fantasy: it grants near-mid fills, ignores stale quotes, and loses
  spread-crossing costs. Share production code between backtest and live so you
  are not testing a fiction.
- **Hedge on state-dependent bands with watchdogs, not a fixed timer and not a
  pure delta band.** A pure band can sit unhedged through a discontinuity; keep
  time-based checks as safety watchdogs.

### 8.5 Discipline that keeps you solvent
- **Hedge the portfolio's net greeks, not each position** (saves spread, avoids
  opposing trades). Strategies keep virtual attribution books; execution is
  consolidated.
- **Keep the dealer signal separable and prove it out-of-sample before it touches
  live hedging.** Do not let an unvalidated latent-state model contaminate your
  cleanest P&L stream (gamma scalping).
- **Respect the latency tier.** You are not an HFT — but the *execution-critical*
  path (fill-driven delta updates, duplicate-order prevention, price collars,
  cancel/replace, kill switches) still must be fast and local. Never put Python
  or a remote DB in that path.

---

*End of plan.*
