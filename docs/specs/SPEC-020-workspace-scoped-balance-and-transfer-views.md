---
status: accepted
owner: iron-burrow
last_reviewed: 2026-07-31
agent_edit_policy: update_when_relevant
---

# SPEC-020 - Workspace-Scoped Balance and Transfer Views

## Purpose

Deliver RFC-003 Phase 4's first Data Lab data views for a selected, watch-only
Workspace member address without expanding the stable machine API.

## Scope

- Authenticated HTML views below `/workspaces/{workspace_id}/addresses/{member_id}`
  on `www.ironburrow.com`.
- Existing balance composition for `eth-mainnet` and `base-mainnet`, and
  existing ERC-20 transfer search for `eth-mainnet` only.
- Account ownership and network-scoped `balances.read` / `transfers.read`
  authorization before upstream work.
- Existing request limits, selectors, partial-result behavior, balance
  evidence, and transfer truncation presentation.

## Non-goals

- `/v1` routes, OpenAPI operations, JSON browser APIs, aggregation across
  Workspace members, price derivation, or a generic block explorer.
- Activity/evidence persistence or agent-facing structured delivery; those
  belong to SPEC-021.

## Security and interfaces

All Workspace routes require an active browser session. State-changing forms
require same-origin and CSRF validation; owned resources are resolved using the
immutable account ID, and cross-account identifiers return `404`. Authenticated
HTML responses are private and no-store. Balance and transfer presenters call
shared application services directly, never Mother `/v1` handlers.

The routes are HTML-only and are documented in `CONTRACTS.md`; they make no
stable machine-facing compatibility promise. `base-mainnet` transfer requests
render an unavailable state without calling an upstream provider.

## Acceptance criteria

1. A signed-in account can view data only for its selected Workspace member.
2. Missing capability grants deny before Bigwig or Price Indexer work.
3. Existing source/evidence and partial/truncation fields are retained in the
   rendered result.
4. Existing `/v1` contracts and OpenAPI remain unchanged.
