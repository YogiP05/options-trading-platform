# Options Trading Platform — Long-Form Build Stage Plan

> **Companion to:** `options-platform-plan.md` (the architecture & strategy
> blueprint). That document says **what** to build and **why**; this document
> says **how to start building it**, in what order, and **how you know a stage
> is actually done**.
>
> **Reading order:** Read `options-platform-plan.md` first (especially §1
> Principles, §2 Sequencing, §8 Developer Notes). This plan repeatedly cites
> those principles as `P1`–`P5`.
>
> **Status:** Pre-build. No code exists yet.
> **Last updated:** 2026-08-10

---

## How to Read This Plan

Each **Stage** is written as a contract:

- **Goal** — the one-sentence outcome.
- **Why now / Depends on** — the principle or prior stage that forces the ordering.
- **Workstreams** — parallelizable tracks of work inside the stage.
- **Deliverables** — concrete artifacts that must exist when the stage ends.
- **Exit Gate** — the *objective, testable* criteria that let you move on. If the
  gate isn't green, the stage is not done, no matter how much code exists.
- **Anti-goals** — things you must consciously NOT do yet (scope discipline).
- **Primary risk** — the thing most likely to silently rot the stage.

Stages are grouped into the six **Phases** already named in the blueprint (§6),
so this plan is a strict refinement of that order, never a contradiction of it.

**Golden rule (from P4):** the backtester and the live system share the same
pricing / surface / signal / hedge code from the very first stage that produces
any of it. We never build a "research version" and a "production version" of the
same math. This constraint shapes almost every stage below.

---

## Stage Map at a Glance

| Phase (blueprint §6) | Stage | Name | Exit gate in one line |
|---|---|---|---|
| Pre-work | S0 | Engineering Foundations | Monorepo + Rust/Python bridge + CI + golden-test harness are green |
| Phase 0 | S0.5 | Walking-Skeleton Spike | Thin end-to-end slice proves the seams (PyO3, shared code, non-blocking exec loop) |
| Phase 0 | S1 | Data Ingestion & Normalized Schema | One internal tick schema, replayable historical + live, quote-level |
| Phase 0 | S2 | Pricing & Greeks Core (Rust) | Reprice book <1 ms; greeks match QuantLib oracle within tolerance |
| Phase 0 | S3 | Arb-Free Vol Surface (SVI/SSVI) | Surface is smooth + arbitrage-free; vanna/charm are stable, not noise |
| Phase 0 | S4 | Transaction-Cost & Fill Model | Quote-level fills with spread/impact; validated vs. real fills |
| Phase 0 | S5 | Event-Driven Backtester | Same code as live; deterministic replay reproduces a known day |
| Phase 0 | S6 | Three-Clock Runtime Skeleton | Execution loop never blocks on analytics; kill switch works |
| Phase 1 | S7 | Delta-Neutral Core: Long Gamma Scalping | Break-even-variance entry + state-dependent bands, proven in backtest |
| Phase 1 | S8 | Portfolio Net-Greek Hedging | Hedge consolidated net greeks, not per-position |
| Phase 1 | S9 | Defined-Loss Short Vol | Scenario-loss sizing + surface-shock stress, hard event exclusions |
| Phase 2 | S10 | Flow Classifier | Trade-direction + multi-leg detection with confidence tags |
| Phase 2 | S11 | Four-Model GEX Ensemble (Shadow) | Publishes P(GEX>0) + flip interval; logged, never live |
| Phase 2 | S12 | Latent Inventory State-Space Model | Posterior over dealer inventory; footprints only validate, never assign sign |
| Phase 3 | S13 | 0DTE Module | Intraday-OI estimator + hourly charm clock |
| Phase 4 | S14 | Economic Activation | Dealer signal improves `J` out-of-sample; bounded live modulation |
| Phase 5 | S15 | Advanced Strategies | Dispersion, relative-value vol, single-name squeeze |

Cross-cutting tracks (X1–X5) run continuously and are described after the stages.

---

# PHASE 0 — Foundations

> P2: "A clean, arbitrage-free volatility/risk foundation must come first." and
> P4: "Backtest realism is make-or-break." Everything in Phase 0 exists so that
> the first strategy in Phase 1 is standing on solid ground. **Do not start
> Phase 1 until every Phase 0 exit gate is green.**

---

## Stage S0 — Engineering Foundations

**Goal:** A monorepo where Rust and Python share one build, one test command, and
one CI pipeline, with a golden-test harness in place before any domain code.

**Why now / Depends on:** Nothing. This is the substrate. Doing it later means
retrofitting the Rust↔Python bridge (PyO3) and the shared-code discipline (P4)
onto an existing mess.

**Workstreams:**
- **Repo & build.** Monorepo layout: `rust/` (core crates), `py/` (research +
  services), `proto/` or schema dir, `docs/`, `infra/`. Cargo workspace + a
  Python packaging setup (uv/poetry). Reproducible dev env (devcontainer/nix or a
  pinned Docker toolchain).
- **PyO3 bridge skeleton.** A trivial Rust function exposed to Python via PyO3,
  imported and called from a Python test. This proves the single-implementation
  path (blueprint §5) end to end before it matters.
- **CI/CD.** Lint (clippy, ruff), format (rustfmt, black), typecheck (mypy on the
  Python side), unit tests both languages, and the golden-test harness — all on
  every PR. Fail the build on any gate.
- **Golden-test harness.** A framework for "record a known-good numerical output,
  assert future runs reproduce it within tolerance." This is how S2/S3 will pin
  greeks and surfaces against QuantLib.
- **Config & secrets.** Typed config loading; no secrets in the repo; a clear
  local-vs-prod config split.

**Deliverables:** Bootable monorepo; green CI running both languages; a PyO3
"hello from Rust" round-trip test; documented `make`/`just` targets for build,
test, lint, bench.

**Exit Gate:**
- `just test` (or equivalent) runs Rust + Python + golden tests and passes.
- A PyO3-exposed Rust function is called from a passing Python test.
- CI blocks a PR that fails lint/type/test.

**Anti-goals:** No trading logic. No pricing math. No data vendors yet. Resist
building "just a little" of the greeks engine here.

**Primary risk:** Skipping the PyO3 bridge and golden harness "to save time,"
then discovering research/live drift (the exact thing P4 forbids) three stages
later.

---

## Stage S0.5 — Walking-Skeleton Spike

**Goal:** A deliberately thin, throwaway-if-needed end-to-end slice that proves
the **architectural seams** before we invest in real domain math: dummy data →
trivial pricing → pass-through backtester → three-clock skeleton, all wired
together.

**Why now / Depends on:** S0. The three things most expensive to discover late
are (1) the PyO3 boundary being awkward, (2) the shared backtest/live code path
(P4) not actually being shareable, and (3) the execution loop blocking on
analytics (§8.5). This spike surfaces all three on day ~3 instead of month ~3.

**Workstreams:**
- **End-to-end seam.** Hard-coded/dummy tick → a stub Rust pricer (returns a
  constant) exposed via PyO3 → a pass-through "strategy" → a trivial event-driven
  backtest run → the same code booted under the three-clock skeleton.
- **Non-blocking proof.** Inject an artificial delay in the analytics clock and
  show the execution clock keeps ticking (§8.5 latency-tier invariant), in
  miniature.
- **Shared-code proof.** The dummy strategy is called by BOTH the backtester and
  the skeleton live loop from one implementation (P4), in miniature.

**Deliverables:** A running end-to-end skeleton with stubbed components; a short
write-up of any seam friction found (feeds the real S2–S6 designs).

**Exit Gate:**
- One dummy tick flows end-to-end: feed → PyO3 pricer → strategy → backtest AND
  skeleton live loop, from shared code.
- An artificial analytics-clock stall does **not** stall the execution loop.

**Anti-goals:** No real pricing, surface, data vendor, or strategy. This is a
seam test, not a feature. It is allowed to be thrown away once S2–S6 replace each
stub.

**Primary risk:** Letting the spike quietly become production — stubs must be
replaced by the real S2–S6 implementations, not blessed as "good enough."

---

## Stage S1 — Data Ingestion & Normalized Schema

**Goal:** All feeds (options, underlying, reference, rates/dividends) normalized
to **one internal tick schema** at the edge, with both a **live** path and a
**replayable historical** path, at **quote level**.

**Why now / Depends on:** S0. Everything downstream consumes this schema. Getting
the schema wrong is the most expensive mistake in the whole build because every
later stage encodes assumptions about it.

**Workstreams:**
- **Schema design.** One canonical event schema for trades, quotes (NBBO +
  depth if available), underlying ticks, OI snapshots, reference data, corporate
  actions, rates/dividends. Versioned. This is the contract the backtester and
  live system both read (P4).
- **Feed handlers.** Adapters that translate vendor formats (Polygon / Databento
  / dxFeed / CBOE per blueprint §4) into the canonical schema. **Do NOT take raw
  OPRA** (blueprint §4 ingestion note).
- **Raw archive + normalized store.** Parquet-on-S3 tick archive partitioned by
  date/underlying; ClickHouse (or equivalent) for time-series; Postgres stubs for
  the relational source of truth. Raw is kept immutable for re-derivation.
- **Replay engine.** Deterministic historical replay that emits the exact same
  event stream the live handlers would — this is the seam the backtester plugs
  into at S5.
- **Data quality.** Gap detection, out-of-order handling, clock/timestamp
  normalization, and a QC report per ingested day.

**Deliverables:** Canonical schema (documented + code); at least one working
vendor feed handler; historical loader writing Parquet; deterministic replay of
one full trading day for one underlying; a data-QC report.

**Exit Gate:**
- One full day of one liquid underlying (e.g. SPX/SPY) is ingested, archived, and
  **replays deterministically** (same bytes/events every run).
- Quote-level data is present (bid/ask, not just trades) — required by P4/§8.4.
- Schema is versioned and documented; a schema change requires a version bump.

**Anti-goals:** No pricing, no surface, no signals. Don't overfit the schema to
one vendor — normalize.

**Primary risk:** Bar-level or trade-only data sneaking in. Quote-level is
non-negotiable (§8.4: "Backtest at quote level or don't bother").

---

## Stage S2 — Pricing & Greeks Core (Rust)

**Goal:** A stateless Rust pricing/greeks engine that reprices a book of a few
thousand contracts in **<1 ms** on an underlying tick and computes first- and
second-order greeks (delta, gamma, vega, theta, **vanna, vomma, charm, speed**).

**Why now / Depends on:** S1 (needs normalized inputs). This is the numerical
heart; the surface (S3) and every strategy depend on it. It is exposed to Python
via PyO3 so research and live use the *same* implementation (blueprint §5).

**Workstreams:**
- **IV solver.** Implement Jäckel "Let's Be Rational" in Rust (blueprint §5) —
  fast, accurate implied-vol inversion.
- **Greeks engine.** Analytic/where-needed-numerical greeks including second
  order. Stateless service design.
- **QuantLib oracle harness.** Wire QuantLib **as an oracle only** (blueprint §5):
  calendars, curves, American exercise, trees/PDEs. Use it purely for regression
  tests via the S0 golden harness — never in the hot path.
- **PyO3 exposure.** Same engine callable from Python for research/backtest.
- **Performance bench.** Criterion (Rust) benchmark asserting the <1 ms reprice
  target under a realistic book size.

**Deliverables:** Rust pricing crate; IV solver; second-order greeks; QuantLib
regression suite green through the golden harness; PyO3 bindings; a perf bench.

**Exit Gate:**
- Greeks match the QuantLib oracle within a documented tolerance across a matrix
  of strikes/maturities/moneyness (golden tests green).
- Reprice of a few-thousand-contract book completes **<1 ms** in the bench.
- Python calls the identical Rust engine (no second pricing implementation).

**Anti-goals:** No calibration/surface fitting yet (that's S3). No calling
QuantLib in any latency-sensitive path (blueprint §5 explicitly forbids).

**Primary risk:** A second pricing implementation quietly appearing in Python
"just for research," which reintroduces drift. One engine, exposed twice.

---

## Stage S3 — Arb-Free Vol Surface (SVI/SSVI)

**Goal:** A calibrated **arbitrage-free** SVI/SSVI volatility surface producing
**smooth** greeks — specifically **stable vanna and charm**, not numerical noise.

**Why now / Depends on:** S2. This is the gate P2/§8.3 name explicitly: "Build
the arb-free SVI/SSVI surface first — it is a prerequisite, not a nicety."
Vanna/charm are surface derivatives; a bad surface makes every dealer-flow
strategy (Phase 2+) trade noise.

**Workstreams:**
- **Fitter.** SVI per-slice + SSVI whole-surface calibration. Enforce **no
  static arbitrage**: no calendar arbitrage, no butterfly (density) arbitrage.
- **Smoothness & stability.** Regularization so second derivatives (→ vanna,
  charm) are stable tick-to-tick, not jumpy.
- **Residual diagnostics.** Per-point residual z-scores and local RMSE by regime
  (feeds Strategy C later): `z = (σ_market − σ_model)/RMSE_local`.
- **Arb-free test suite.** Automated checks for arbitrage violations across a
  battery of historical days; golden tests pin surfaces.
- **Hot/warm split.** Fit runs on the analytics clock (blueprint §4); the hot
  path reads the latest published surface, never blocks on a refit.

**Deliverables:** Surface fitter (Rust hot eval + Python calibration research);
arbitrage-free validator; vanna/charm stability report; residual/RMSE outputs;
golden surface tests.

**Exit Gate:**
- Fitted surfaces pass **no-calendar-arb and no-butterfly-arb** checks across a
  historical battery.
- **Vanna and charm are demonstrably stable** (bounded tick-to-tick variation on
  quiet data) — the concrete test that §8.3 demands.
- Surface eval in the hot path is fast enough not to stall the execution clock.

**Anti-goals:** No trading on the surface yet. No dealer/GEX engine (Phase 2).
Don't ship a surface that fits well but violates arbitrage — that's worse than
useless for second-order greeks.

**Primary risk:** Chasing fit quality (low RMSE) at the expense of arbitrage-free
smoothness, yielding pretty surfaces with garbage vanna/charm — silently
poisoning all of Phase 2.

**Review policy — DOUBLE REVIEW.** A subtle arbitrage/smoothness bug here
poisons every Phase 2+ signal. This stage's PR gets reviewed by **two different
vendors** (not the implementer), not one.

---

## Stage S4 — Transaction-Cost & Fill Model

**Goal:** A realistic **quote-level** fill and cost model: spread crossing, depth,
market impact, and discrete-hedge slippage — validated against real fills.

**Why now / Depends on:** S1 (quote data). §8.4: "Transaction costs determine
whether a gross edge survives." Gamma scalping's edge lives or dies here, so the
cost model must exist *before* the strategy that depends on it (S7).

**Workstreams:**
- **Fill model.** No near-mid fantasy fills. Cross the spread, respect stale
  quotes, model partial fills and depth.
- **Impact/slippage.** Size-dependent impact for both the option legs and the
  underlying hedge.
- **Break-even-variance inputs.** Produce the friction terms S7 needs: premium
  spread, hedge spread + impact, expected IV change, event/gap reserves, skew.
- **Validation.** Compare modeled fills to any available real execution data;
  calibrate.

**Deliverables:** Fill/cost model shared by backtest and live; documented
friction decomposition feeding the break-even-variance test; a validation report.

**Exit Gate:**
- Backtest fills reflect actual spread crossing and stale-quote handling (no
  mid-fills on a spread-crossing order).
- The model outputs every friction term S7's entry test consumes.
- Modeled costs are validated against real fills within a documented tolerance.

**Anti-goals:** No strategy logic. Don't hard-code a flat per-contract commission
and call it a cost model.

**Primary risk:** Optimistic fills. A backtest that grants near-mid fills makes
every gamma-scalp look profitable and loses money live (§8.4).

---

## Stage S5 — Event-Driven Backtester

**Goal:** An event-driven, **quote-level** backtester that runs the **exact
production pricing/surface/signal/hedge code**, swapping only clock, feed, broker,
and fill model.

**Why now / Depends on:** S1–S4. This is the embodiment of P4. It must exist
before any strategy so strategies are validated on the same code that trades.

**Workstreams:**
- **Event loop.** Deterministic event-driven engine reading the S1 replay stream.
- **Code-sharing seam.** Pricing (S2), surface (S3), and cost model (S4) plug in
  unchanged. Only clock/feed/broker/fill differ from live.
- **Determinism & reproducibility.** Same inputs → same outputs, seeded, with run
  manifests (data version, code SHA, config) for auditability.
- **`nautilus_trader` evaluation.** Evaluate adopting it as the unified
  backtest/live engine (blueprint §5) vs. a bespoke loop; decide and document.
- **Reporting.** P&L, cost attribution, hedge trace, greek history per run.

**Deliverables:** Backtester that reproduces a known day deterministically;
run-manifest system; a build-vs-buy decision on `nautilus_trader`; standard
reporting.

**Exit Gate:**
- A backtest run **reproduces a known historical day deterministically**.
- The backtester provably calls the **same** pricing/surface/cost code paths as
  the (skeleton) live system — verified, not assumed.
- Every run emits a manifest (data + code + config versions).

**Anti-goals:** No strategies yet — the backtester runs a trivial pass-through so
the harness is proven before real strategies load. No live trading.

**Primary risk:** A "research-only" reimplementation creeping in to make backtests
faster/easier, breaking the shared-code invariant that is the entire point.

---

## Stage S6 — Three-Clock Runtime Skeleton

**Goal:** The live runtime skeleton implementing the **three clocks** (blueprint
§4), where the execution/risk loop **never blocks** on surface refits, GEX, or
any statistical model, and an independent kill switch works.

**Why now / Depends on:** S2 (greeks in the hot path). Establishes the runtime
shape (P3 latency tier, §8.5) so strategies drop into the right clock.

**Workstreams:**
- **Execution/Risk clock.** Event-driven, sub-ms–tens-of-ms, **local Rust state
  only — no Python, no remote DB in this path** (§4, §8.5). Positions, greeks,
  pre-trade risk, delta hedges, kill switches.
- **Analytics clock.** 100 ms–minutes: surface, flow, GEX, signals, regime.
- **Reconciliation/Research clock.** Intraday–daily: OI reconciliation, fits,
  attribution, retraining.
- **Message bus.** NATS/JetStream wiring (blueprint §5) between clocks.
- **Safety layer.** Independent execution-layer kill switch + risk limits that
  live **below/outside** the strategy layer (blueprint §5). Duplicate-order
  prevention, price collars, cancel/replace.
- **Postgres source of truth.** Positions/orders/fills/P&L reconciled off the hot
  path.

**Deliverables:** Running three-clock skeleton (no strategy); kill switch +
risk-limit layer; bus wiring; Postgres reconciliation; a latency report for the
execution path.

**Exit Gate:**
- Execution loop demonstrably **does not block** when the analytics clock stalls
  or a surface refit runs long.
- The **kill switch halts order flow** independently of the strategy layer
  (tested by fault injection).
- No Python and no remote DB sit in the execution-critical path (verified).

**Anti-goals:** No alpha. This is plumbing and safety. Don't put convenience
Python or a DB call in the hot path "temporarily."

**Primary risk:** Latency-tier violations (§8.5) — a remote DB or Python call
slipping into the fill-driven path — which no strategy can later undo.

**PHASE 0 COMPLETE when S0–S6 gates are all green. Only then start Phase 1.**

---

# PHASE 1 — Delta-Neutral Core

> The workhorse strategies (blueprint §3 A–C). This is the cleanest P&L stream;
> P5/§8.5 insist we keep it uncontaminated by unvalidated dealer signals.

---

## Stage S7 — Long Gamma Scalping

**Goal:** Implement the workhorse (blueprint §3.A): enter when **forecast realized
variance exceeds executable break-even variance after all frictions**, hedge on a
**state-dependent no-trade band** with fixed-time watchdogs.

**Why now / Depends on:** All of Phase 0. This is the first real strategy and the
canonical test of whether the foundation is sound.

**Workstreams:**
- **Entry test.** Forecast RV vs. **executable break-even variance after
  frictions** (from S4) — explicitly **NOT** "realized vol > displayed ATM IV"
  (§8.1). Build the RV forecaster.
- **Hedge controller.** State-dependent band
  `b_t = f(Γ, spread, depth, σ̂_forecast, jump risk, time, inventory, regime)`
  (§3.A). Widen for cost/impact; narrow for gamma/liquidity/expected short-horizon
  vol. **Keep fixed-time watchdogs** so a pure band never sits unhedged through a
  discontinuity.
- **Shadow `dealer_regime` seam.** Per P5/§2, the hedge controller accepts an
  **optional** `dealer_regime` context now and logs baseline vs. conditioned
  recommendations — but nothing feeds it yet (that's Phase 2/4).
- **Backtest validation.** Run through S5 at quote level with S4 costs.

**Deliverables:** RV forecaster; break-even-variance entry; state-dependent hedge
controller with watchdogs; the shadow `dealer_regime` interface (inert);
quote-level backtest results with cost attribution.

**Exit Gate:**
- Entry uses executable break-even variance (frictions included), verified by
  code review + tests — **not** the folk RV-vs-ATM-IV comparison.
- Hedging uses state-dependent bands **plus** time watchdogs.
- The `dealer_regime` seam exists and logs shadow deltas but has **zero** live
  effect (P5).
- Positive/understood risk-adjusted result in a quote-level backtest, with costs
  from S4 fully attributed.

**Anti-goals:** Do NOT wire any dealer/GEX signal into hedging (Phase 4 only). Do
NOT code the "RV > ATM IV" shortcut.

**Primary risk:** Over-hedging microstructure noise and coding the folk entry
test — both systematically overstate edge (§8.1).

---

## Stage S8 — Portfolio Net-Greek Hedging

**Goal:** Consolidate hedging to the **portfolio's net greeks**, not per position;
strategies keep virtual attribution books, execution is consolidated (§8.5).

**Why now / Depends on:** S7. Once one strategy hedges, multi-position hedging
must consolidate to save spread and avoid opposing trades.

**Workstreams:**
- **Net-greek aggregator.** Real-time net delta/gamma/vega across all positions.
- **Virtual attribution books.** Each strategy sees its own P&L/greeks; execution
  nets them.
- **Consolidated hedge execution.** One hedge order stream, not N competing ones.

**Deliverables:** Net-greek hedging in the execution clock; attribution bookkeeping;
backtest showing reduced hedge cost vs. per-position hedging.

**Exit Gate:**
- Hedger acts on **net** greeks; opposing per-strategy hedges are demonstrably
  netted out.
- Attribution books reconcile to the consolidated execution.
- Measured spread savings vs. naive per-position hedging.

**Anti-goals:** No per-position hedge trades reaching the broker. No dealer signal.

**Primary risk:** Attribution drift — virtual books that don't reconcile to real
fills, corrupting P&L truth.

---

## Stage S9 — Defined-Loss Short Vol (VRP Harvesting)

**Goal:** Implement short gamma / variance-risk-premium harvesting (§3.B) with
**defined-loss** structures, **scenario-loss** sizing, and **surface-shock**
stress — never parallel IV bumps.

**Why now / Depends on:** S3 (surface for stress), S7/S8 (hedging). Adds the other
side of the vol book under strict risk discipline.

**Workstreams:**
- **Entry test.** Sell when implied variance materially exceeds the full forecast
  distribution of realized variance **and jumps**, after reserving for tail risk.
- **Defined-loss structures.** Straddle/strangle/iron with **wings** (§3.B).
- **Risk unit = scenario loss.** Size by scenario loss, **not premium collected**;
  never call it "income" internally (§8.1).
- **Surface-shock stress.** Stress with **skew steepening / term twists**, NOT
  parallel IV bumps (§3.B, §8.1). Hard event exclusions / reduced sizing; hedge
  escalation near concentrated short strikes; max dollar-gamma limits.

**Deliverables:** Short-vol strategy with defined-loss structures; scenario-loss
sizer; surface-shock stress engine; event-exclusion rules; backtest with tail
diagnostics.

**Exit Gate:**
- All short-vol positions are **defined-loss** (wings enforced).
- Sizing is driven by **scenario loss** with surface-shock stress (skew/term),
  and parallel-IV-bump stress is explicitly absent as the sizing basis.
- Event exclusions and max dollar-gamma limits are enforced in code.

**Anti-goals:** No naked short vol. No "income" framing. No parallel-bump risk
sizing.

**Primary risk:** The catastrophic left tail (§8.1) — under-reserving because the
stress test used parallel IV bumps instead of the surface moves that actually
kill short-vol books.

**PHASE 1 COMPLETE when S7–S9 gates are green and the delta-neutral core is a
clean, measured P&L stream.**

---

# PHASE 2 — Positioning Analytics (Shadow Mode Only)

> Blueprint §3 E–I and §2. Everything here is **logged, never live** (P1, P5).
> The whole phase runs in shadow mode; it earns live authority only in Phase 4.

---

## Stage S10 — Flow Classifier

**Goal:** Infer trade direction (customer-vs-dealer initiated) with **mandatory
multi-leg / complex-order detection**, every classification **confidence-tagged**.

**Why now / Depends on:** S1 (trade+quote data). The GEX ensemble's signed-flow
model (§3.E model 2) depends on this; getting it wrong biases positioning.

**Workstreams:**
- **Trade-direction inference.** Classify aggressor side from trade vs. quote
  context.
- **Multi-leg detection.** **Mandatory** (§3.E) detection of spreads, paired
  prints, complex orders — raw call volume alone is insufficient (§3.J, §8.2).
- **Confidence tagging.** Every classification carries a calibrated confidence.

**Deliverables:** Flow classifier service (analytics clock); multi-leg detector;
confidence-calibrated outputs; a validation report vs. any labeled data.

**Exit Gate:**
- Classifier detects multi-leg/complex orders (not just single prints).
- Every output is confidence-tagged.
- Accuracy validated where ground truth exists; failure modes documented.

**Anti-goals:** No GEX aggregation yet. No trading on flow. Don't treat raw call
volume as directional intent (§8.2).

**Primary risk:** Naive tick-rule direction inference that misreads spreads and
paired prints, feeding a biased sign into GEX.

---

## Stage S11 — Four-Model GEX Ensemble (Shadow)

**Goal:** Aggregate dealer gamma over a spot grid via the **four-model ensemble**
(§3.E), publishing **P(GEX(S)>0 | data)** and a **flip interval** — logged in
shadow mode, never driving trades.

**Why now / Depends on:** S3 (surface/greeks), S10 (flow). This is the core
positioning engine, built as a *model with confidence* per P1.

**Workstreams:**
- **GEX aggregation.** `GEX(S) = Σ_j q_j·OI_j·M_j·Γ_j(S,σ_j,T_j)·S²·0.01` (§3.E).
- **Four models, each confidence-tagged (§3.E):**
  1. Naive OI (fixed dealer-sign convention).
  2. Signed-flow inventory (from S10; multi-leg aware).
  3. Scenario ensemble (GEX under multiple sign assignments → report a **range**).
  4. Calibrated latent-state (placeholder here; realized in S12).
- **Probabilistic outputs.** Publish `P(GEX>0)`, a **flip interval** (e.g.
  `S* ∈ [5170,5205]`), net dealer gamma/vanna/charm notional, pin scores
  `P_k = OI_k·Γ_k·e^(−α|S−k|)·L_k` (§3.G).
- **Shadow logging.** Feed the inert `dealer_regime` seam from S7; log baseline vs.
  conditioned hedge recs, differences, counterfactual cost/P&L, confidence,
  freshness (§2 shadow-mode table).

**Deliverables:** GEX engine on the analytics clock; three live ensemble members +
a slot for S12; probabilistic flip intervals + pin scores; shadow telemetry
wired to the hedge controller.

**Exit Gate:**
- Engine publishes **probabilities and intervals**, never a point estimate or a
  single flip line (P1, §8.2).
- `dealer_regime` is logged in **shadow mode** with full counterfactual telemetry
  and has **zero** live hedge effect (§2).
- Scenario model reports a **range** across sign assignments.

**Anti-goals:** Do NOT trade on GEX. Do NOT hard-code "above flip = revert, below
= trend" (§8.2 — it's a hypothesis to learn). Do NOT present GEX as an oracle
(P1).

**Primary risk:** Treating an assumed dealer sign as truth and leaking the signal
into live hedging before Phase 4.

**Review policy — DOUBLE REVIEW.** Highest-stakes correctness stage; reviewed by
**two different vendors** than the implementer.

---

## Stage S12 — Latent Inventory State-Space Model

**Goal:** A calibrated state-space **posterior over dealer inventory** (blueprint
§4), where **footprints validate/weight** models but **never assign the sign**
(that would be circular — §7).

**Why now / Depends on:** S11 (it's ensemble model #4). Upgrades the ensemble from
heuristics to a formal latent-state estimator (§7 resolution).

**Workstreams:**
- **State-space model.** `I_t = I_{t−1} + F_t + ε_t`;
  `Y_t = h(I_t,S_t,σ_t,L_t) + η_t` (§4). `I` = latent dealer inventory, `F` =
  classified flow (S10), `Y` = observed price/vol/hedge-flow footprints.
- **Observations.** Next-day OI changes as **delayed** observations; realized
  market behavior as **noisy contemporaneous** evidence.
- **Non-circularity guard.** Footprints are a **validation/emission target only**,
  never used to assign sign (§7, §4 — "that would be circular"). Encode this as an
  explicit architectural constraint + test.
- **Intraday OI posterior.** Treat intraday OI as a **latent inventory posterior**,
  not "yesterday's OI + buys − sells" (§4): estimate customer-vs-dealer,
  opening-vs-closing, single-vs-complex, paired prints, exercise/assignment.

**Deliverables:** State-space estimator feeding ensemble model #4; intraday-OI
posterior; documented non-circularity guarantee + test; calibration running on the
reconciliation clock.

**Exit Gate:**
- Posterior over dealer inventory is produced with credible intervals.
- **Footprints demonstrably do not assign sign** (guard test passes) — sign comes
  from the state-space prior/flow, footprints only validate/weight (§7).
- Next-day OI changes reconcile as delayed observations.

**Anti-goals:** No live trading. No circular calibration (low RV → "selling" →
high GEX). Still shadow mode.

**Primary risk:** Circular inference — using market-behavior footprints to both
assign and confirm the dealer sign, which §7 flags as the central danger.

**Review policy — DOUBLE REVIEW.** The non-circularity guard is the crux;
reviewed by **two different vendors** than the implementer, with one review
focused solely on the footprints-never-assign-sign invariant.

**PHASE 2 COMPLETE when S10–S12 gates are green and positioning analytics run in
full shadow with confidence + intervals — and still touch nothing live.**

---

# PHASE 3 — 0DTE Module

---

## Stage S13 — 0DTE Module

**Goal:** A first-class 0DTE module (blueprint §4) where the **intraday-OI
estimator is the whole game** (OI resets daily), gamma is huge but
intraday-transient, and **charm runs on an hourly clock**.

**Why now / Depends on:** S12 (intraday-OI posterior), S11 (GEX). 0DTE has its own
dynamics that generalize the positioning engine to a daily-reset regime.

**Workstreams:**
- **Intraday-OI estimator.** Since OI resets daily, this estimator *is* the signal
  (§4). Built on the S12 posterior.
- **Hourly charm clock.** Scheduled charm forecasts at open/midday/final-hour/
  overnight boundary (§3.I): `ΔQ_hedge ≈ −DealerCharm·Δt`.
- **Vanna flow.** `ΔQ_hedge ≈ −DealerVanna·Δσ`, modeling the **whole surface
  move**, not a parallel shift (§3.H).
- **Transient-gamma handling.** Represent gamma as large but intraday-transient.
- **Testable-prior discipline.** Per §7, treat 0DTE suppression/acceleration as a
  **testable prior**, not a given (Cboe caution that simple narratives overstate
  the effect).

**Deliverables:** 0DTE intraday-OI estimator; hourly charm forecaster; vanna-flow
estimator (whole-surface); shadow evaluation of the 0DTE positioning signal.

**Exit Gate:**
- Intraday-OI estimator runs and is validated against EOD OCC OI as it settles.
- Charm forecasts fire on the hourly schedule at the named session boundaries.
- Vanna/charm hedge-demand estimates use whole-surface moves (§3.H/§3.I), and the
  0DTE effect is measured as a hypothesis, not assumed (§7).

**Anti-goals:** No live 0DTE trading yet (activation is Phase 4). No parallel-shift
vanna. Don't assume the 0DTE narrative is true — measure it.

**Primary risk:** Over-crediting the 0DTE dealer-gamma narrative (§7) and building
transient intraday OI as if it were a simple running sum.

---

# PHASE 4 — Economic Activation

---

## Stage S14 — Economic Activation (Gated)

**Goal:** Promote the dealer signal from shadow to **bounded live hedge
modulation**, but **only** after it improves the objective `J` **out-of-sample**,
via the staged rollout in §2.

**Why now / Depends on:** S11–S13 shadow evidence. This is the moment P5/§2 have
been protecting: the signal earns live authority empirically, not by design.

**Objective (blueprint §2):**
```
J = E[hedged P&L] − λ₁·Var(P&L) − λ₂·E[cost] − λ₃·E[tail loss]
```

**Staged rollout (blueprint §2):**
1. Baseline cost-aware hedger (no dealer signal) — the control.
2. Shadow GEX-conditioned hedger (logs only) — already built (S11).
3. Small-capital randomized / matched comparison.
4. Bounded modulation (e.g. ±10–20% band width).
5. Larger control authority only after stable out-of-sample evidence.

**Workstreams:**
- **Out-of-sample evaluation.** Measure `J` for shadow-conditioned vs. baseline on
  held-out data; require a real improvement.
- **Randomized/matched live test.** Small-capital A/B or matched comparison
  (rollout step 3).
- **Bounded modulation.** Let the dealer regime modulate **band width / urgency /
  gamma selection** within hard bounds (±10–20% initially), never full control.
- **Guardrails.** Signal freshness checks, confidence floors, and automatic
  demotion to baseline if the signal degrades.

**Deliverables:** OOS evaluation of `J`; small-capital comparison results; bounded
live modulation controlled by confidence/freshness; auto-demotion guardrail.

**Exit Gate:**
- Dealer-conditioned hedging shows a **statistically real out-of-sample
  improvement in `J`** vs. the baseline (the §2 activation gate).
- Live authority is **bounded** (±10–20% band-width modulation), not full control.
- Auto-demotion to baseline triggers on stale/low-confidence signals.

**Anti-goals:** No unbounded control authority. No activation without OOS proof
(P5). Never hard-wire the noisy regime into the clean gamma-scalp P&L (§8.5).

**Primary risk:** Injecting regime noise into the cleanest P&L stream (P5, §8.5) —
the exact failure the whole shadow-first sequence exists to prevent.

---

# PHASE 5 — Advanced Strategies

---

## Stage S15 — Advanced Strategies

**Goal:** Add the higher-complexity strategies (blueprint §3 C/D/J): relative-value
vol, dispersion, and single-name squeeze detection — each built on the now-proven
foundation.

**Why now / Depends on:** Everything. Blueprint §3.D explicitly stages dispersion
**after** the single-underlying engine (many surfaces, earnings gaps, corporate
actions, rebalances, high transaction counts).

**Workstreams:**
- **Relative-value straddles/strangles (§3.C).** Trade surface residual z-scores
  off the arb-aware SVI/SSVI fit (S3). Remember **"vega neutral is NOT risk
  neutral"** — neutralize the risk matching the thesis (gamma/vanna/charm/jump),
  not just delta.
- **Dispersion (§3.D).** Explicitly a **correlation trade**: short index variance +
  long weighted constituents = short implied correlation (blows up as corr→1 in
  selloffs). Start with top 20–50 constituents; constrained optimizer minimizing
  residual delta/vega/sector/earnings/liquidity; futures for index delta, shares
  for constituent delta.
- **Squeeze detection (§3.J).** Rank single names on estimated negative dealer
  gamma + customer call buying + near-dated call concentration above spot + short
  interest/borrow utilization + thin book + rising IV with positive price/IV
  co-movement + high hedge demand vs. ADV. Express via **call spreads** or small
  long-gamma. Raw call volume alone is insufficient (§3.J).

**Deliverables:** Relative-value vol strategy; dispersion engine (multi-surface,
corporate-action-aware); squeeze scanner + expression logic; backtests for each.

**Exit Gate:**
- Relative-value trades neutralize risk per the thesis (not just delta/vega).
- Dispersion handles multiple surfaces, earnings gaps, corporate actions, and
  rebalances without blowing the transaction-count/latency budgets.
- Squeeze detection uses the full multi-factor rank (not raw call volume) with
  clear invalidation conditions.

**Anti-goals:** Don't attempt dispersion before the single-underlying engine is
solid (§3.D). Don't treat vega-neutral as risk-neutral (§3.C).

**Primary risk:** Correlation-blowup carry in dispersion (§3.D) and treating
conditional pinning/squeezes as physical laws (§8.2).

---

# Cross-Cutting Tracks (run continuously across all phases)

These are not stages; they are disciplines that must be present from S0 onward and
grow with the system.

### X1 — Model & Data Versioning (P1)
GEX is a model; its "assumptions, confidence, and historical revisions must be
first-class, versioned data — never an oracle" (P1). Every surface fit, GEX
estimate, and signal carries a model version + confidence and is reproducible from
versioned inputs (tie into S5 run manifests). Historical revisions are retained.

### X2 — Risk, Safety & Kill Switches (§5, §8.5)
The independent execution-layer kill switch and risk limits (S6) are maintained and
fault-tested every phase. Pre-trade risk, price collars, duplicate-order prevention,
and max dollar-gamma limits are never bypassed by a new strategy. Safety lives
below/outside the strategy layer, always.

### X3 — Observability & P&L Attribution
Dashboards, alerts, and P&L attribution (blueprint §4 final row) grow with each
strategy. Virtual attribution books (S8) reconcile to real fills continuously.
Shadow-mode counterfactual telemetry (§2) is retained for the eventual `J`
evaluation in S14.

### X4 — Backtest/Live Parity Audits (P4)
Periodically prove that backtest and live still share code paths (the S5
invariant). Any divergence is a release blocker. Re-run the "reproduce a known
day" determinism test on every meaningful change.

### X5 — Research→Production Discipline (§5, §8)
One pricing implementation (Rust, exposed via PyO3), QuantLib as oracle only, no
second math library in the hot path. Every new signal ships as shared code, not a
research notebook that later gets "productionized."

---

# Agent Assignments & Review Policy

> Who builds what, and who checks it. The governing rule (never relaxed):
> **review is always performed by a DIFFERENT vendor than the implementer.**

### Active roster (installed on this machine)
- **`claude_code`** — broad, multi-file scaffolding, refactors, systems wiring,
  and test-heavy work. Gets the "broad" stages.
- **`codex`** — narrow, well-scoped, numerically-precise units (IV solver, cost
  model, a single strategy's entry math). Gets the "deep and tight" stages.
- **`cursor`** — third vendor; picks up implementation when the other two are
  busy and keeps the review rotation independent.

### Pending activation (NOT installed yet)
- **`agy` (Gemini / antigravity CLI)** — a genuinely different vendor family from
  Claude/GPT/Cursor, so it is the **ideal independent reviewer** for the
  double-review stages (S3, S11, S12). **Blocked:** the `agy` CLI is not on PATH,
  so it cannot be dispatched today. The moment it is installed and resolves,
  slot it in as the preferred second reviewer on the double-review stages and as
  a fourth implementer in the rotation.

### Per-stage assignments

| Stage | Work character | Implementer | Reviewer(s) |
|---|---|---|---|
| **S0** Engineering Foundations | broad scaffolding | `claude_code` | `codex` |
| **S0.5** Walking-Skeleton Spike | thin end-to-end seam | `claude_code` | `cursor` |
| **S1** Data Ingestion & Schema | multi-file + schema design | `claude_code` | `cursor` |
| **S2** Pricing & Greeks Core (Rust) | tight numerical Rust | `codex` | `claude_code` |
| **S3** Arb-Free Vol Surface | math-heavy, multi-workstream | `claude_code` | **double:** `codex` + `cursor` |
| **S4** Transaction-Cost & Fill Model | scoped numerical | `codex` | `cursor` |
| **S5** Event-Driven Backtester | broad systems wiring | `claude_code` | `codex` |
| **S6** Three-Clock Runtime Skeleton | broad systems + safety | `claude_code` | `cursor` |
| **S7** Long Gamma Scalping | scoped strategy math | `codex` | `claude_code` |
| **S8** Portfolio Net-Greek Hedging | systems aggregation | `claude_code` | `codex` |
| **S9** Defined-Loss Short Vol | scoped strategy + stress | `codex` | `cursor` |
| **S10** Flow Classifier | scoped inference | `codex` | `claude_code` |
| **S11** Four-Model GEX Ensemble | broad, multi-model | `claude_code` | **double:** `codex` + `cursor` |
| **S12** Latent Inventory State-Space | deep numerical/statistical | `codex` | **double:** `claude_code` + `cursor` |
| **S13** 0DTE Module | scoped, builds on S12 | `codex` | `cursor` |
| **S14** Economic Activation | broad eval + guardrails | `claude_code` | `codex` |
| **S15** Advanced Strategies | broad, multi-surface | `claude_code` | `cursor` (per sub-strategy) |

**Notes:**
- Assignments are **defaults, tuned per task at dispatch** — cost/difficulty can
  move a stage, and `args.model` is set per dispatch (cheaper/faster for
  scaffolding; strongest models for S3, S7, S12).
- **Double review** applies to **S3, S11, S12** only — the stages where a subtle
  math/circularity bug silently poisons everything downstream. Two different
  vendors (never the implementer) review the same diff + contract. Once `agy`
  is installed it becomes the preferred second reviewer on these.
- Within a stage, independent workstreams may go to different vendors in
  parallel (e.g. S2's IV solver vs. QuantLib oracle harness). The table shows
  the lead implementer.
- Reviewers receive **only the diff + acceptance contract**, never the
  implementer's worktree. Only the implementer opens a PR; the human merges.

---

# Architecture Decision Records (ADR) Log

> The blueprint defers several build-vs-buy calls. Each is captured as a short
> ADR (context → options → decision → consequences) in `docs/adr/`, authored at
> the stage that forces the choice, so the rationale lives in-repo. These are
> prose deliverables (polly can draft them; a sub-agent formalizes).

| ADR | Decision | Forced at |
|---|---|---|
| ADR-001 | Monorepo layout & Rust/Python build toolchain | S0 |
| ADR-002 | Canonical tick schema & versioning policy | S1 |
| ADR-003 | Data vendor (Polygon vs. Databento vs. dxFeed/CBOE) | S1 |
| ADR-004 | Time-series store (ClickHouse vs. kdb+) | S1 |
| ADR-005 | Message bus (NATS/JetStream vs. Kafka) | S6 |
| ADR-006 | Backtester: bespoke event loop vs. `nautilus_trader` | S5 |
| ADR-007 | Broker/execution path (IBKR → FIX) | before live |
| ADR-008 | Objective-function weights (λ₁, λ₂, λ₃ in `J`) | S14 |

---

# Long-Lead External Dependencies (start in parallel with S0)

> These are account/subscription/procurement lead times, **not code**. Kick them
> off alongside S0 so they never gate the build at S1.

- **Quote-level data vendor subscription** (Databento / Polygon). S1 onward is
  blocked without quote-level history (P4/§8.4). Longest lead item — start now.
- **Reference underlying decision** (e.g. SPX/SPY) so S1–S7 have a concrete
  target. A one-line call, but everything downstream assumes it.
- **Broker account** (Interactive Brokers) for eventual paper/live execution
  (S6/S14). Not needed for Phase 0 backtesting, but the account-opening lead
  time is real.
- **Infra accounts** (S3 bucket for the tick archive, compute for calibration).

---

# Ticketization & Registry

> Before any stage is dispatched, its workstreams are broken into per-PR tickets
> in `.polly/registry.json` — one worktree + one implementation sub-agent + one
> PR per ticket. This keeps stages parallelizable, gives each cross-review a
> crisp diff + contract, and lets the human track progress. polly maintains the
> registry; polly never merges — each PR is the deliverable and the human merges.

---

# Suggested Delivery Cadence & Parallelism

- **Strictly sequential gates:** S0 → S1 → {S2, S4 partially parallel} → S3 → S5 →
  S6. Phase 0 is mostly a chain because each stage's exit gate is the next stage's
  precondition. S4 (cost model) can start once S1 lands quote data, in parallel
  with S2/S3.
- **Phase 1** (S7→S8→S9) is sequential but each stage is small once the foundation
  holds.
- **Phase 2** (S10→S11→S12) can begin as soon as S3 (surface) and S1 (flow data)
  exist — it does **not** need Phase 1 complete, because it's shadow-only. Running
  Phase 2 in parallel with Phase 1 is reasonable **provided** the `dealer_regime`
  seam stays inert (P5).
- **Phase 3** (S13) needs S12. **Phase 4** (S14) needs S11–S13 shadow evidence.
  **Phase 5** (S15) needs the whole foundation and the single-underlying engine.

**Hard ordering invariants that must never be violated:**
1. No strategy before the arb-free surface (S3) and quote-level backtester (S5) —
   P2, P4.
2. No dealer signal touches live hedging before S14's OOS `J` gate — P5, §2.
3. No Python / remote DB in the execution-critical path, ever — §8.5.
4. Backtest and live share pricing/surface/signal/hedge code, from S5 onward — P4.

---

# When We Start Building (recommended first moves)

1. **Kick off S0 now.** Monorepo + PyO3 bridge + CI + golden-test harness. This is
   pure engineering with no domain risk and unblocks everything.
2. **In parallel, lock the S1 schema on paper.** The canonical tick schema is the
   most expensive thing to change later; design and review it before writing feed
   handlers.
3. **Stand up the QuantLib oracle harness early** (part of S0/S2) so S2 and S3 have
   a regression target from day one.
4. **Do not let any strategy code exist until S3 and S5 gates are green.** That is
   the single most important discipline in this whole plan (P2 + P4).

---

## Delegation note (for the build itself)

When we move from planning to building, each stage's workstreams map cleanly onto
independent, individually-reviewable tickets (see **Ticketization & Registry**)
— each is one worktree + one implementation sub-agent opening its own PR,
cross-reviewed by a different vendor (two vendors on the S3/S11/S12 double-review
stages) before the human merges.

**Active coding workers on this machine:** `claude_code`, `codex`, and `cursor`
(the `claude`, `codex`, and `cursor-agent` CLIs are installed). The review
rotation runs among these three — enough for real different-vendor review (e.g.
codex implements, claude_code reviews).

**Not installed:** `opencode`, `hermes`, and `pi` are absent (we have
deliberately decided **not** to pursue `pi`). `agy` (Gemini / antigravity CLI) is
intended but its binary is **not yet on PATH**, so it cannot be dispatched today;
once installed it joins as a fourth vendor and becomes the preferred second
reviewer on the double-review stages.

No build work has been dispatched yet — this document is the plan we execute
against.

---

*End of build stage plan.*
