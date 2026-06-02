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
