---
status: accepted
owner: iron-burrow
last_reviewed: 2026-07-31
agent_edit_policy: update_when_relevant
---

# SPEC-021 - Workspace Activity and Evidence

## Purpose

Implement RFC-003 Phase 5: an append-only, account-owned Workspace activity
and evidence log shared by the human Workspace timeline and a private
agent-facing structured export.

## Scope

- Persist Workspace lifecycle, address, and label mutations as immutable
  activity events.
- Persist authorized balance and ERC-20 transfer observation outcomes with a
  versioned normalized evidence snapshot. Complete, partial, and unavailable
  outcomes are retained; invalid and unauthorized requests are not.
- Render `GET /workspaces/{workspace_id}/activity` for the owning browser
  session and export `GET /workspaces/{workspace_id}/activity.json` for an
  account-owned bearer key with `workspace.activity.read`.
- Use newest-first cursor pagination with `limit` (default 50, maximum 100)
  and an optional `before=wae_*` cursor.

## Non-goals

- A `/v1` route, OpenAPI operation, agent event ingestion, raw provider-payload
  archival, data aggregation, delegation/client management, or treasury work.
- Backfilling events that occurred before this migration, deletion/retention
  controls, or organization-shared Workspaces.

## Security and persistence

Events have an opaque `wae_*` identifier, Workspace foreign key, event type,
actor kind, schema version, JSON payload, and occurrence timestamp. The
database rejects direct updates and deletes; Workspace hard deletion is not a
product operation. Payloads never contain API-key secrets or session material.

`workspace.activity.read` is an account-only capability. Active IBAccounts
and their active account-owned keys receive it through the Phase 5 migration
and future account/key issuance. Legacy and anonymous-demo keys do not. The
JSON export authenticates and consumes the existing per-key quota before
checking that the key's active IBAccount owns the requested Workspace; a
missing or non-account grant is `403 capability_not_granted`, while a
non-owned Workspace is `404`.

Observation snapshots preserve canonical `network_slug`, request inputs,
source identity, block/time evidence, available quote timestamps, result
status, and partial/truncation indicators. Mother does not store raw Bigwig or
Price Indexer payloads and does not recalculate their data.

## Interfaces and acceptance

The HTML timeline remains a private no-store Workspace page. The `.json`
export returns `{ "ok": true, "workspace": ..., "events": [...],
"next_before": ... }`; it is an evolving web-product transport, not a stable
machine API or `/v1` compatibility promise.

Phase 5 is complete when mutations and observations are persisted, cross-account
access is denied, the database immutability rule is tested, existing `/v1` and
OpenAPI behavior is unchanged, and Postgres migration/regression checks pass.
