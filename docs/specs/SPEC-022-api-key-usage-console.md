---
status: draft
owner: iron-burrow
last_reviewed: 2026-07-31
agent_edit_policy: update_when_relevant
---

# SPEC-022 - API-Key Usage Console

## Purpose

Implement a deliberately small authenticated Data Lab page that lets a Mother
API key holder inspect key metadata, limits, recent attributable usage, and
capability-level aggregates, without changing existing `/v1/*` contracts.

## Scope

- Askama-rendered usage console routes under `/app`.
- API key subject-kind (`human|agent|machine`) persistence and operator
  visibility.
- Credential-derived server-side console session.
- Attributable usage events for existing protected Beta capabilities.
- Per-capability daily aggregate for console reporting.

## Non-goals

- Any change to existing `/v1/*` route URLs, auth contract, request/response
  shape, quota semantics, or OpenAPI behavior.
- Public self-service key management.
- Customer identity platform (registration, passwords, OAuth, org/member
  model).
- Billing, subscriptions, credits, or x402.
- `/scan/*` feature delivery or explorer behavior.

## Dependencies

- RFC-003 (El Vasco architecture and `/v1` vs `/app` boundary).
- SPEC-013 (capability authorization foundation).
- SPEC-014 (Mother web runtime and homepage shell).

## Security relevance

- Raw API keys are never persisted after verification, never rendered in HTML,
  and never stored in browser storage.
- Session cookie must be `Secure`, `HttpOnly`, `SameSite=Lax`, path-scoped to
  `/app`, and carry only an opaque random session identifier.
- Session storage persists only a hash of session identifier, bound
  `api_key_id`, issuance, expiry, and invalidation timestamps.
- Access failures are generic and indistinguishable across malformed, unknown,
  disabled, revoked, expired, and disabled-consumer states.
- POST routes use CSRF defense (token or strict origin checks).

## Key-holder model

For this slice, the authenticated console principal is the key holder:

```text
one active API key <-> one api_consumer <-> one key holder
```

Rules:

- `api_consumer` may retain revoked historical keys.
- No more than one active key per consumer.
- Issuance fails if the consumer already has an active key.
- Rotation is revoke-then-issue.
- `api_consumer.category` (`friend|partner|public|internal`) remains issuance
  metadata and is distinct from `api_key.subject_kind`.

`api_key.subject_kind` is immutable and one of `human`, `agent`, `machine`.

## Routes and behavior

Required route behavior:

```text
GET  /                    -> public landing page (SPEC-014)
GET  /app/access          -> key-entry form
POST /app/access          -> validate bearer key; create session; redirect
GET  /app/usage           -> authenticated key-scoped usage console
POST /app/logout          -> invalidate session; redirect
```

Route boundary rules:

- `/v1/*` and `/health` behavior remains unchanged.
- `/app/usage` may return only the authenticated session key's own data.
- HTML rendering failures return generic HTML `500` and do not alter JSON API
  behavior.

## Session lifecycle

- Absolute lifetime: 8 hours.
- Idle timeout: 30 minutes.
- Session invalidates on logout.
- Session invalidates if bound API key is no longer active.
- Failed access attempts are not usage events.

## Usage event model

Activity in this spec means attributable Mother API access events, not on-chain
transactions.

For each valid authenticated request to protected Beta routes, record a
best-effort event after response completion with fields:

```text
event_id
api_key_id
capability
requested_at
http_status
outcome_class
request_id (when supplied)
```

Never persist in usage events:

- raw API key, key hash, Authorization header
- request body, account address, token selector, client reference
- IP address, provider identity, or Bigwig internals

Initial capability labels:

```text
api.balances.single
api.balances.bulk
api.erc20_transfers.search
```

Rate-limited valid-key requests are recorded as `rate_limited` outcome.
Requests rejected before principal attribution are not recorded.
Event-write failure is observable via logs/metrics and must not change an
already completed `/v1` response.

Retention:

- Raw usage events: 90 days.
- Existing daily key usage counters remain quota source of truth.
- Add capability/day aggregate for efficient 30-day capability summary reads.

## Console data contract

`GET /app/usage` renders:

- key prefix, label, subject-kind, state, expiry, last-use timestamp
- configured per-minute and per-day limits
- 30-day daily usage table from existing counters
- 30-day per-capability totals from capability/day aggregate
- 100 most recent attributable events
- explicit `Scan usage` unavailable panel (no scan capability in this slice)

The console is operational telemetry for key holders, not billing-grade
metering and not a full forensic log.

## Persistence requirements

Implementation must add:

1. `api_key.subject_kind` constrained column with safe backfill.
2. Partial unique constraint enforcing one active key per consumer.
3. `api_key_console_session` table with hashed session identifiers and
   expiry/invalidation fields.
4. Append-only `api_key_usage_event` table plus key/time indexes.
5. Capability/day aggregate table keyed by key, UTC day, capability.

All durable data remains in Postgres under `mother_api` schema.

## Application boundaries

- Route-to-capability mapping is explicit and kept beside protected route
  definitions.
- Web handlers call focused application services and presenters.
- No direct coupling from usage-console handlers to Bigwig, DIS, or price
  indexer adapters.

## Test requirements

- Generic sign-in failure behavior across malformed/unknown/inactive/expired
  key states.
- Session expiry, idle timeout, logout invalidation, and key deactivation
  invalidation.
- Session-to-key isolation (no cross-key leakage).
- No raw key exposure in templates, redirects, logs, or static assets.
- Event privacy assertions for excluded fields.
- Non-regression for existing `/v1` response envelopes and quota behavior.

## Acceptance criteria

1. Existing `/v1` contracts and OpenAPI output remain unchanged.
2. Issued keys show `subject_kind` in operator and console surfaces.
3. Console access is limited to the authenticated key's own usage data.
4. Raw API keys are never rendered or persisted client-side.
5. Usage panel correctly distinguishes current API activity vs unavailable scan
   capability.
6. Event persistence failures are observable but do not mutate completed public
   API outcomes.

## Suggested implementation phase

Phase 3, after SPEC-014 establishes web runtime shell and before broader
account-backed Workspace slices.
