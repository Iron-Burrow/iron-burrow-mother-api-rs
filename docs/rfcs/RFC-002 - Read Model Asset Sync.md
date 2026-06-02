---
status: draft
owner: iron-burrow
last_reviewed: 2026-06-02
agent_edit_policy: update_when_relevant
---

# RFC-002 - Read Model Asset Sync

Status: Draft

Created: 2026-06-02

## Summary

This RFC proposes analyzing how `iron-burrow-read-model` should discover
the Mother API asset catalog and supported quote currencies it needs for
refresh scheduling.

No endpoint, route, response shape, or public contract is accepted by this
RFC. In particular, this RFC does not promise `/v1/assets/active` or any
other exact URL.

## Problem

`iron-burrow-read-model` needs a canonical way to know which assets and
quote currencies should be refreshed. Without an explicit sync mechanism,
the read model can drift from the catalog Mother API owns, or it can become
coupled to product-facing response shapes that were not designed for refresh
orchestration.

## Current State

Mother API owns the canonical `mother_api.global_asset`, `network`, and
`asset_chain_map` catalog. The public `GET /v1/assets` endpoint lists active
assets for product clients and includes client-facing fields such as
canonical paths and optional price enrichment.

That public list is active-only, but it is not shaped as a read-model sync
feed. It does not currently define supported quote currencies for refresh
planning, and its response shape is part of the public product contract.

## Candidate Approach

One possible approach is a minimal read-model sync surface that returns the
catalog entries and quote currencies needed to build refresh attempts. This
surface would be intentionally smaller than `GET /v1/assets` and would avoid
price data, chain-specific addresses, and unrelated operational metadata.

The exact transport, path, visibility, auth model, response shape, and
ownership boundary are unresolved and must be decided before implementation.

## Alternatives To Analyze

- Let `iron-burrow-read-model` read Mother API catalog tables directly.
- Add an internal-only Mother API endpoint for read-model sync.
- Publish catalog changes through a message or event stream.
- Reuse the existing public `GET /v1/assets` endpoint and add separate quote
  currency discovery.

## Non-Goals

- Do not add or promise `/v1/assets/active`.
- Do not change the current `CONTRACTS.md` surface.
- Do not move price availability, derivation, or historical price ownership
  into Mother API.
- Do not make `GET /v1/assets` serve read-model-specific fields unless a
  later accepted proposal chooses that direction.

## Open Questions

- Should read-model sync be public-safe, internal-only, or private to the
  deployment network?
- Where should supported quote currencies be defined and versioned?
- Should the sync surface be pull-based, event-driven, or both?
- What freshness and failure semantics does `iron-burrow-read-model` need?
- Does this need an implementation spec after the RFC is accepted?
