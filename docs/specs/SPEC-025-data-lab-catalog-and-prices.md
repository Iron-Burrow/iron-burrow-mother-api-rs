---
status: accepted
owner: iron-burrow
last_reviewed: 2026-08-06
agent_edit_policy: update_when_relevant
---

# SPEC-025 - Data Lab Catalog and Prices

## Purpose

Deliver authenticated Data Lab catalog, asset, network, and price presenters
over Mother catalog services and the read-only Price Indexer boundary.

## Scope

HTML and private JSON presenters expose catalog identity, network mappings,
latest prices, and bounded existing price signals. Enrichment failures are
honest partial results. No presenter calculates prices, stats, trends, or
series locally and none creates a `/v1` or OpenAPI operation.
