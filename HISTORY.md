---
status: active
owner: iron-burrow
last_reviewed: 2026-06-02
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
