---
status: draft
owner: iron-burrow
last_reviewed: 2026-08-07
agent_edit_policy: update_when_relevant
---

# SPEC-032 - Portfolio Strategy Simulation Lab

## Purpose

Provide an authenticated, browser-native `/lab/portfolio-simulation` study
that answers what an initial USD investment would have become under one
curated strategy during a bounded historical period. It is an evolving human
product transport, not a `/v1` operation or public compatibility promise.

## Model and ownership

A portfolio is the time-indexed state of cash, spot assets, and protocol
positions; it does not make decisions. A strategy is compiled Mother Rust,
identified by immutable `strategy_slug` and `strategy_version`, that creates
the initial portfolio and may later emit deterministic operations. Callers
cannot submit allocations, rules, code, addresses, providers, or calldata.

Mother owns the simulator, strategy registry, evidence normalization,
performance calculations, and private run records. Price Indexer remains the
read-only price and FX authority. Bigwig remains the controlled archive-chain
read boundary. This creates no service, public API, execution capability, or
Python dependency.

## Initial study

The browser form accepts a positive decimal `initial_capital`, `USD`, inclusive
`start_date`/`end_date`, and a compiled strategy. Dates are evaluated as UTC
`[start 00:00, end + 1 day 00:00]` boundaries and may span at most 366 days.
The first registry contains `btc-hold@v1`, `eth-hold@v1`, and
`aave-usdc-supply@v1`.

Each validated run is stored as an account-owned append-only `psr_*` record:
versioned input, normalized price/protocol evidence, operations, snapshots,
result, outcome, and SHA-256 digest. Stored evidence never includes raw
provider payloads or secrets. A prior result remains inspectable even if an
upstream history is later corrected.

Results expose `complete`, `partial`, `unsupported`, or `failed` outcome;
strategy and engine version; initial/final value; absolute and percentage
return; annualized return only for complete periods of 30 days or more;
maximum drawdown only for a complete daily series; operations; snapshots; and
evidence digest. Gross return separates price appreciation and yield where
supported. Gas, swaps, reward tokens, and separately attributable fees are
explicitly unmodeled rather than assumed zero.

## Evidence and protocol behavior

Mother uses only Price Indexer's bounded absolute-date daily series contract.
The existing relative `window` signal is not eligible for a simulation. Missing
start or final price makes the run `unsupported`; missing interior evidence or
identified carry-forward evidence makes it `partial` and suppresses metrics
requiring a complete series. Price source status, timestamp, type, derivation
metadata, and coverage are retained as normalized evidence.

`aave-usdc-supply@v1` resolves a canonical Ethereum block at every snapshot
boundary through Bigwig and reads Aave V3 reserve normalized income using the
verified protocol registry. The shared index-at-block primitive also continues
to power the standalone realized-yield Lab; this SPEC does not turn that
pair-only study result into a generic protocol abstraction.

Strategy evaluation follows: initial state → historical evidence → strategy
evaluation → modeled operations → updated state → snapshots. Future strategies
may consume only evidence available at their decision time. Historical data
never silently supplies a strategy decision.

## Interface and deferrals

`GET /lab/portfolio-simulation` renders the form; its CSRF-protected POST
creates a run at `/lab/portfolio-simulation/runs`; an owning account reads its
result at `/lab/portfolio-simulation/runs/{psr_id}`. All use browser sessions,
`lab.read`, Askama, and private no-store responses. The result includes an
accessible table and server-rendered SVG timeline; no JavaScript framework is
introduced.

Comparisons, staked ETH, Morpho vaults, Curve positions, multi-position
portfolios, active rebalancing, user-authored rules, historical fee/gas models,
and Python tools are deferred. Morpho and Curve historical evidence require
accepted registry/adapter scope; current Mother implements only the Aave V3
realized-yield adapter.

## Acceptance

Tests cover date and capital validation, deterministic spot and Aave math,
missing/carry-forward evidence outcomes, strategy-version capture, immutable
and account-isolated persistence, CSRF/authorization, and accessible result
rendering. `cargo test`, `make test-db-postgres`, and `make smoke-db-migrate`
must pass. No `/v1`, OpenAPI, or `CONTRACTS.md` change is made.
