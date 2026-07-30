---
status: draft
owner: iron-burrow
last_reviewed: 2026-07-30
agent_edit_policy: update_when_relevant
---

# SPEC-013 - Capability Authorization Foundation

## Purpose

Define the first, compatibility-safe authorization foundation required by
RFC-003. The implemented slice introduces registered capability IDs and
enforces the intersection of the existing API-consumer compatibility owner and
the individual API key for current private-Beta routes.

## Scope

- `balances.read` and `transfers.read` capability registry entries.
- Owner and API-key grant persistence, status, expiry, revocation, scope, and
  active-lookup indexes.
- Domain authorization request/context/decision types and table-driven tests.
- Route mapping for the existing balance and ERC-20 transfer routes.
- Additive backfill and issuance defaults that preserve existing issued keys.
- Stable `403 capability_not_granted` contract/OpenAPI response.

## Non-goals

- IBAccounts, email identities, browser sessions, anonymous keys, self-service
  key management, plans, payments, or new API routes.
- Arbitrary resource scopes, quota redesign, basic/custom RPC, Otterscan,
  Bitcoin, Lightning, Scan, or Lab.
- Reclassifying `api_consumer` as an IBAccount.

## Dependencies

- RFC-003.
- Existing `api_consumer`, `api_key`, API-key policy/usage migrations, and
  protected Beta middleware from SPEC-010.
- Binding route/OpenAPI contract in `CONTRACTS.md`.

## Security relevance

The slice prevents a valid credential from implying every operation. Owner
grants are an upper bound, key grants only narrow it, expired/revoked grants do
not permit access, and a denied request consumes no accepted-request quota or
calls an upstream service. Logs include only non-secret identifiers/prefixes.

## Domain changes

- `Capability`: `balances.read`, `transfers.read`.
- `NetworkScope`: `*` or canonical `network_slug`.
- `CapabilityGrant`, `AuthorizationRequest`, `AuthorizationContext`, and
  `AuthorizationDecision`.
- The current owner is explicitly a compatibility owner; planned account/key
  model specs replace it with `IBAccount` without permitting a broader key.

## Forward-compatible ownership notes

This foundation intentionally does not implement `IBAccount`, anonymous keys,
or `Client` ownership. It must, however, remain compatible with later models:

1. Anonymous access where `owner = NULL` and capability scope remains explicitly
   constrained.
2. `IBAccount`-owned keys as the normal product path.
3. Future `Client`-owned or organization-owned keys with unambiguous ownership
   invariants.

No future ownership model may allow a key to exceed its owner boundary.

## Database changes

Migration `0009_legacy_api_key_capabilities.sql` creates the schemas:

- `mother_api.capability` (`id` PK, description, timestamp).
- `mother_api.api_consumer_capability_grant` natural PK
  `(consumer_id, capability_id, network_scope)`.
- `mother_api.api_key_capability_grant` natural PK
  `(api_key_id, capability_id, network_scope)`.

Both grants have `active|revoked` status, expiry/revocation/audit timestamps,
canonical scope checks, foreign keys, and active lookup indexes. The embedded
reference-data catalog declares the capability rows and reconciles every
existing consumer and key with exactly the two legacy capabilities during
`mother-api db apply`. Issuance applies the same default to newly
operator-issued legacy keys.

## Public interfaces

Existing paths, request/response bodies, bearer header, `401`, `429`, and
`503` behavior remain unchanged. A valid, intentionally narrowed key now gets:

```json
{"ok":false,"error":{"code":"capability_not_granted","message":"The API key is not authorized for this operation."}}
```

with HTTP `403`. This is documented in `CONTRACTS.md` and generated OpenAPI.
No capability management interface is public.

## Acceptance criteria

- Existing issued keys retain both existing operation capabilities after
  `mother-api db apply` runs migrations and required reference data.
- Owner denial overrides a key grant; a key can narrow an owner grant.
- Grant status and network scope domain behavior are table-tested.
- Balance-only key reaches balance validation but gets `403` before transfer
  handler execution.
- Denial does not increment accepted usage or call Bigwig.
- OpenAPI includes `403` and the contract error catalogue remains synchronized.
- Full Rust test suite and Postgres-backed migration tests pass.

## Suggested implementation phase

Phase 1. The first vertical slice is implemented in this change and remains
subject to RFC/SPEC review before later phases rely on its schema as a durable
IBAccount model.
