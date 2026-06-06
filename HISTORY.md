---
status: active
owner: iron-burrow
last_reviewed: 2026-06-06
agent_edit_policy: append_only
---

# HISTORY.md

Append-style project change log for notable Mother API contract,
implementation, and documentation changes.

## 2026-06-02

- Implemented optional `/v1/assets/{slug}` asset-detail enrichments via
  `include=priceStats,priceTrend,priceSeries`, backed by the existing
  price-indexer Query Layer client.
- Added partial enrichment failure reporting with `signals` null values and
  `enrichment_errors` while preserving the base asset-detail `200 OK`
  response when the asset exists.
- Updated `CONTRACTS.md` with the new asset-detail query parameters, optional
  signal fields, and enrichment error codes.
- Hardened the demo path with clearer asset-detail contract examples,
  enrichment failure observability, partial-failure regression coverage, and a
  Maria UI smoke checklist.

## 2026-06-03

- Drafted `SPEC-004-dis-polymarket-prediction-routes.md`: DIS-backed Mother API
  public/demo routes `GET /v1/predictions/fifa-world-cup/winner` and
  `GET /v1/predictions/fifa-world-cup/{country}` for the World Cup 2026 demo.
- Framed SPEC-004 as a sibling of SPEC-001: both are Mother API → DIS
  integrations (Aave V3 realized yield vs. Polymarket World Cup predictions),
  reusing the same DIS client foundation.
- Kept the spec in `draft`; no public endpoint is implemented yet and
  `CONTRACTS.md` is intentionally unchanged until the spec is accepted and the
  routes are built.
- Added the internal DIS prediction snapshot client slice: typed
  Polymarket snapshot DTOs, DIS config/state wiring, bounded retry/timeout
  behavior, and error classification without exposing public prediction
  routes.
- Implemented the SPEC-004 public/demo prediction routes:
  `GET /v1/predictions/fifa-world-cup/winner` and
  `GET /v1/predictions/fifa-world-cup/{country}`.
- Added app-level fake-DIS route tests for winner success, country
  normalization, unknown query parameter handling, sanitized public responses,
  decimal-string preservation, missing DIS config, and unsupported subjects.
- Updated `CONTRACTS.md` and `README.md` with the new prediction endpoint
  shapes and public error codes.
- Hardened SPEC-004 public error mapping with explicit provider
  unavailable/timeout coverage, sanitized DIS failure envelopes, missing-DIS
  graceful degradation, and the generic `internal_error` response constructor.
- Finalized the SPEC-004 judge-demo contract path with live Polymarket-implied
  wording, exact public error examples, local/dev curl smoke commands, and
  implemented-status spec language.

## 2026-06-04

- Wired `DIS_BASE_URL`, `DIS_REQUEST_TIMEOUT_MS`, and
  `DIS_RETRY_MAX_ATTEMPTS` into the Mother API Compose environment so the
  implemented prediction routes can reach DIS on `iron-burrow-net`.
- Added `checks.dis` to `/v1/status` as a config/client availability signal
  with `configured`, `not_configured`, and `invalid_config` states.
- Changed `checks.price_indexer` on `/v1/status` from a reserved
  `not_connected` placeholder to a config/client availability signal with
  `configured`, `not_configured`, and `invalid_config` states.

## 2026-06-06

- Forwarded `/v1/assets/{slug}?quoteCurrency=...` to the price-indexer latest
  price lookup so the base `price` block can return direct or derived prices in
  `USD`, `MXN`, `USDC`, or `BTC`.
- Preserved `USD` as the default and kept currency conversion and derivation
  entirely inside price-indexer.
