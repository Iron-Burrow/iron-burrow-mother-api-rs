---
status: draft
owner: iron-burrow
last_reviewed: 2026-07-01
agent_edit_policy: update_when_relevant
---

# SPEC-010 - Beta API-Key Access Service

Draft implementation specification for adding the smallest runtime API-key
access layer needed by the private Beta Mother API surface.

This specification is not current runtime truth. [CONTRACTS.md](../../CONTRACTS.md)
currently says inbound API keys, bearer authentication, and rate limiting are
out of scope. If this spec is accepted and implemented, the same change that
protects public routes must update `CONTRACTS.md`, OpenAPI, examples, smoke
checks, and `HISTORY.md`.

## Summary

Mother API already has schema foundations for API consumers and API keys:

- `mother_api.api_consumer`
- `mother_api.api_key`

Those tables were introduced as schema-only groundwork. Migrations and
reference data still create no real customer records and no real API keys.

This spec proposes the next beta access-control slice:

- issue real API keys through an operator-only CLI;
- authenticate beta public routes with `Authorization: Bearer <api_key>`;
- enforce coarse per-key limits;
- track basic daily usage;
- keep `/health` public;
- avoid public key-management routes, billing, OAuth, JWT, and self-service
  customer dashboard behavior.

The slice is intentionally narrow. It adds beta access control, not a general
identity platform.

## Authoritative Context

- `CONTRACTS.md` is authoritative for implemented public behavior and currently
  documents no inbound authentication.
- Accepted [SPEC-008](SPEC-008-balance-endpoint-beta-contract-hardening.md)
  keeps the Beta public route surface small and defers API-key protection to a
  later auth-specific spec.
- Draft [SPEC-009](SPEC-009-reference-data-and-migration-lifecycle.md) defines
  the database lifecycle boundary: schema migrations may create API-key tables,
  but migrations and reference data must not create real issued keys.
- Migration `0007_api_key_adoption.sql` already creates constrained
  `api_consumer` and `api_key` tables and comments that future auth must verify
  the full presented secret by hashing it and comparing it to `key_hash`.
- `mother-api` currently exposes `serve` and `db` lifecycle commands only. This
  spec adds operator-only admin CLI commands; it does not add HTTP admin routes.

## Goals

- Allow an operator to create or reuse a real API consumer.
- Allow an operator to issue a real API key for that consumer.
- Store only a key prefix and SHA-256 hash, never the raw key.
- Authenticate beta public API routes with `Authorization: Bearer <api_key>`.
- Keep `/health` public and unauthenticated.
- Enforce simple per-key request limits.
- Track daily usage per API key.
- Let operators revoke, list, and inspect usage for issued keys.
- Keep public auth and limit failures stable, documented, and non-enumerating.

## Non-Goals

This slice must not implement:

- OAuth.
- JWT authentication.
- Public API-key management endpoints.
- Customer self-service dashboards.
- Billing, pricing plans, x402, or payment boundaries.
- Key rotation workflow beyond issuing a replacement key and revoking the old
  key.
- Per-endpoint scopes or permissions.
- Redis or distributed rate limiting.
- Full audit-event streams.
- Real customer records or real API keys in migrations or reference data.
- Old TypeScript gateway admin, explorer, account, tracked-token, or price
  route sprawl.

## Existing Schema

This slice reuses the existing tables:

- `mother_api.api_consumer`
- `mother_api.api_key`

The existing rules remain valid:

- `api_consumer.slug` is normalized lowercase kebab case.
- `api_consumer.category` is one of `friend`, `partner`, `public`, or
  `internal`.
- `api_consumer.status` is one of `active`, `disabled`, or `archived`.
- `api_key.key_prefix` is a non-secret lookup hint.
- `api_key.key_hash` stores the 32-byte SHA-256 digest of a cryptographically
  random high-entropy API-key secret.
- `api_key.hash_algorithm` remains `sha256`.
- `api_key.status` is one of `active`, `disabled`, or `revoked`.
- Revoked keys must have `revoked_at`; non-revoked keys must not.
- This SHA-256 pattern is valid only for generated high-entropy API keys. It
  must not be reused for passwords or human-generated low-entropy tokens.

## New Schema

Add one schema migration for policy and daily usage tables. These are runtime
tables, not reference data.

### `mother_api.api_key_policy`

Stores coarse limits for each API key.

```sql
create table if not exists mother_api.api_key_policy (
  api_key_id uuid primary key
    references mother_api.api_key (id) on delete cascade,

  requests_per_minute integer not null default 60,
  requests_per_day integer not null default 5000,

  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),

  constraint api_key_policy_requests_per_minute_non_negative
    check (requests_per_minute >= 0),

  constraint api_key_policy_requests_per_day_non_negative
    check (requests_per_day >= 0),

  constraint api_key_policy_timestamps_sane
    check (updated_at >= created_at)
);
```

`0` means no requests are allowed for that limit. The implementation must update
`updated_at` when an operator changes a policy row.

### `mother_api.api_key_usage_daily`

Stores daily usage counters per API key. `usage_date` is the UTC calendar date
derived from request time.

```sql
create table if not exists mother_api.api_key_usage_daily (
  api_key_id uuid not null
    references mother_api.api_key (id) on delete cascade,

  usage_date date not null,

  accepted_requests bigint not null default 0,
  rate_limited_requests bigint not null default 0,
  successful_responses bigint not null default 0,
  client_error_responses bigint not null default 0,
  server_error_responses bigint not null default 0,

  last_used_at timestamptz,

  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),

  primary key (api_key_id, usage_date),

  constraint api_key_usage_daily_counts_non_negative
    check (
      accepted_requests >= 0
      and rate_limited_requests >= 0
      and successful_responses >= 0
      and client_error_responses >= 0
      and server_error_responses >= 0
    ),

  constraint api_key_usage_daily_timestamps_sane
    check (
      updated_at >= created_at
      and (last_used_at is null or last_used_at >= created_at)
    )
);
```

Counter updates must use atomic `insert ... on conflict ... do update`
statements or an equivalent transaction-safe pattern. A daily limit check must
not allow concurrent accepted requests to exceed the stored daily limit.

## API-Key Format

Generated API keys must use this presented format:

```text
ib_live_<random_prefix>.<random_secret>
```

Example shape:

```text
ib_live_8f3k2m9q1z7p.zEoQ7JqJ8V8bS0...
```

The full key is shown only once at issuance time. The stored values are:

```text
key_prefix = "ib_live_8f3k2m9q1z7p"
key_hash   = sha256(full_presented_key)
```

Requirements:

- Generate the random prefix and secret with an operating-system CSPRNG.
- Keep enough entropy in the secret that online guessing is infeasible.
- Normalize the stored prefix to satisfy the existing `api_key.key_prefix`
  constraint.
- Use a constant-time comparison or a database equality check over the full
  SHA-256 digest; never compare raw presented secrets.
- Never store, log, return in errors, commit in tests, or persist the raw key in
  snapshots.

Implementation may add minimal cryptography/randomness dependencies needed for
generation, SHA-256 hashing, and constant-time comparison. Those dependencies
must be justified in the implementation PR.

## Operator CLI

This slice adds operator-only admin commands to the `mother-api` binary. These
commands use `DATABASE_URL`, run outside the public HTTP surface, and do not
create public admin endpoints.

### Issue API Key

```sh
mother-api admin api-key issue \
  --consumer-slug first-customer \
  --display-name "First Customer" \
  --category partner \
  --label "beta access key" \
  --requests-per-minute 60 \
  --requests-per-day 5000 \
  --expires-at 2026-09-30T00:00:00Z
```

Behavior:

- Create the consumer if it does not exist.
- Reuse the consumer if it already exists.
- Reject a reuse attempt when existing consumer category or display name
  conflicts with supplied arguments, unless a future flag explicitly permits an
  update.
- Create a new API key for the consumer.
- Create a matching `api_key_policy` row in the same transaction.
- Print the full API key exactly once.
- Print the key prefix.
- Do not print the key hash.
- Do not persist the raw key.

### Revoke API Key

```sh
mother-api admin api-key revoke --key-prefix ib_live_8f3k2m9q1z7p
```

Behavior:

- Find the key by prefix.
- Set `api_key.status = 'revoked'`.
- Set `api_key.revoked_at = now()`.
- Update `api_key.updated_at`.
- Succeed idempotently when the key is already revoked.
- Never require or print the raw key.

### List API Keys

```sh
mother-api admin api-key list --consumer-slug first-customer
```

Behavior:

- List API keys for the consumer.
- Show prefix, label, status, expiry, created time, and last used time.
- Never show raw keys.
- Never show key hashes.

### Show Usage

```sh
mother-api admin api-key usage --consumer-slug first-customer --days 30
```

Behavior:

- Show daily usage for keys belonging to the consumer.
- Include accepted requests, rate-limited requests, successful responses,
  client errors, server errors, and last used time.
- Never show raw keys.
- Never show key hashes.

## Authentication Middleware

Protected beta routes must require:

```http
Authorization: Bearer <api_key>
```

Request flow:

1. Read the `Authorization` header.
2. Require exactly one `Bearer` credential.
3. Parse the API-key prefix from the presented key.
4. Hash the full presented key with SHA-256.
5. Find a matching `api_key` by `key_prefix` and `key_hash`.
6. Require `api_key.status = 'active'`.
7. Require the linked `api_consumer.status = 'active'`.
8. Require `expires_at is null or expires_at > now()`.
9. Load the key policy.
10. Enforce per-minute and per-day limits.
11. Attach an `ApiKeyPrincipal` to request extensions.
12. Continue to the handler.

The request principal should include:

```rust
ApiKeyPrincipal {
    api_key_id,
    consumer_id,
    consumer_slug,
    consumer_category,
    key_prefix,
    key_label,
}
```

Authentication depends on Postgres. If the database is required for protected
routes and unavailable, protected requests must fail with a sanitized `503`
public error rather than bypassing authentication.

## Route Policy

The following route remains public:

```text
GET /health
```

In `PUBLIC_API_SURFACE=beta`, externally usable beta API routes must require an
API key:

```text
POST /v1/balances
POST /v1/balances/bulk
POST /v1/erc20-transfers/search
```

`POST /v1/erc20-transfers/search` remains controlled by
`ERC20_TRANSFERS_ENABLED`. When the transfer-search route is disabled, it should
keep the route-surface behavior documented by `CONTRACTS.md` for that release.

This spec does not require changing Alpha-mode public route behavior. If a later
release protects Alpha routes too, that broader change must be documented in the
contract update.

No public API-key management routes are added in this slice.

## Limit Enforcement

This beta slice uses simple limits:

- Per-minute limit: in-memory limiter keyed by `api_key_id`.
- Per-day limit: Postgres counter in `api_key_usage_daily`.

The per-minute limiter assumes a single running Mother API instance for beta.
Distributed rate limiting is explicitly out of scope. The implementation and
deployment notes must make that single-instance assumption visible.

When a request exceeds a limit:

- return `429 Too Many Requests`;
- increment `rate_limited_requests`;
- do not call the protected route handler;
- do not increment `accepted_requests`;
- do not reveal whether the minute or day limit value is sensitive, unless the
  contract explicitly documents a public response field or header.

## Usage Tracking

When a request passes authentication and quota checks:

- Increment `accepted_requests`.
- Update `api_key.last_used_at`.
- Update `api_key_usage_daily.last_used_at`.

After the response is produced, increment exactly one response counter:

- `successful_responses` for `2xx` and `3xx`;
- `client_error_responses` for `4xx`;
- `server_error_responses` for `5xx`.

The middleware must avoid double-counting a request. If response-finalization
tracking cannot be made reliable in the first implementation, that gap must be
called out before acceptance rather than silently omitted.

## Public Error Behavior

Authentication and limit failures must use stable Mother API JSON error
envelopes and must be documented in `CONTRACTS.md` before release.

Required auth cases:

- Missing `Authorization` header.
- Malformed `Authorization` header.
- Unsupported auth scheme.
- Multiple or ambiguous credentials.
- Malformed API-key format.
- Unknown API key.
- Disabled API key.
- Revoked API key.
- Expired API key.
- Disabled or archived API consumer.
- Database unavailable during protected-route authentication.

Required limit cases:

- Per-minute limit exceeded.
- Per-day limit exceeded.

External behavior:

- Missing, malformed, unsupported, unknown, disabled, revoked, expired, and
  disabled-consumer credentials return the same public `401 Unauthorized`
  shape.
- Limit failures return `429 Too Many Requests`.
- Database unavailability returns a sanitized `503 Service Unavailable`.
- Public messages must not reveal whether prefix parsing, hash lookup, key
  status, consumer status, or expiry caused the `401`.

Candidate public error codes:

| HTTP | Code | Meaning |
| ---- | ---- | ------- |
| 401 | `unauthorized` | The request lacks a valid active API key. |
| 429 | `rate_limited` | The valid API key exceeded a request limit. |
| 503 | `database_unavailable` | Mother API could not authenticate the request because Postgres is unavailable. |

The implementation PR may choose narrower names only if `CONTRACTS.md` is
updated consistently and enumeration resistance is preserved.

## Logging Rules

Logs may include:

- `consumer_slug`
- `api_key_id`
- `key_prefix`
- route
- status code
- request duration
- limit outcome

Logs must not include:

- raw API keys;
- key hashes;
- full `Authorization` headers;
- full request bodies solely for auth debugging.

## OpenAPI and Documentation

The implementation must update public documentation in the same change as any
runtime contract change:

- `CONTRACTS.md` auth column and error code table;
- OpenAPI security scheme and protected route requirements;
- success and error examples for protected beta routes;
- smoke-test documentation for authenticated beta calls;
- `HISTORY.md` release entry.

The docs must remain honest that Mother API is early, bounded, and
production-minded. They should describe a private beta access model, not a
self-service customer platform.

## Implementation PR Breakdown

### PR 1 - Schema and Repository Support

- Add `api_key_policy` and `api_key_usage_daily` migration.
- Add repository methods for key lookup, policy lookup, policy creation,
  revocation, last-used updates, and daily counter updates.
- Add Postgres-backed migration and constraint tests.
- Keep migrations and reference data free of real customer records and real API
  keys.

### PR 2 - Key Generation and Operator CLI

- Add minimal key generation, hashing, and prefix parsing.
- Add `mother-api admin api-key issue`.
- Add `mother-api admin api-key revoke`.
- Add `mother-api admin api-key list`.
- Add `mother-api admin api-key usage`.
- Add CLI parser tests and Postgres-backed command tests.
- Ensure raw keys are printed only at issuance and are never persisted.

### PR 3 - Authentication Middleware

- Add API-key authentication middleware for beta protected routes.
- Attach `ApiKeyPrincipal` to request extensions.
- Add route tests for missing, malformed, unsupported, unknown, disabled,
  revoked, expired, disabled-consumer, database-unavailable, and valid-key
  cases.
- Keep `/health` public.

### PR 4 - Limits and Usage Tracking

- Add in-memory per-minute limit enforcement.
- Add atomic per-day limit enforcement through Postgres counters.
- Add response-class usage counters.
- Add tests for accepted, rate-limited, successful, client-error, server-error,
  and last-used updates.
- Document the single-instance per-minute limiter assumption.

### PR 5 - Contracts, OpenAPI, Docs, and Smoke Checks

- Update `CONTRACTS.md` for beta auth requirements, security scheme, and public
  auth/limit errors.
- Update OpenAPI paths and examples.
- Update smoke-test docs and scripts for issuing a key, calling protected
  routes, revoking the key, and inspecting usage.
- Add `HISTORY.md` entry.
- Run required verification.

## Testing Requirements

Add or update tests for:

- generated key format, prefix normalization, entropy length, hash length, and
  absence of raw-key persistence;
- CLI issue, revoke, list, and usage behavior;
- consumer reuse and conflicting consumer arguments;
- policy row creation and policy validation;
- middleware rejection for every auth failure case;
- valid key acceptance and `ApiKeyPrincipal` attachment;
- `/health` remaining public;
- beta protected routes requiring auth;
- daily accepted counter increments;
- daily quota rejecting excess requests without handler execution;
- per-minute limit rejecting excess requests;
- rate-limited requests being counted;
- successful, client-error, and server-error response counters;
- `last_used_at` updates;
- sanitized logs and command output omitting raw keys and hashes.

Postgres-backed tests must follow the repository rule: do not make plain
`cargo test` run migrations or mutate arbitrary databases. Use the disposable
test database path behind `make test-db-postgres`.

## Verification

Required local verification:

```sh
cargo fmt
cargo test
git diff --check
```

Required Postgres verification:

```sh
make test-db-postgres
```

Required smoke verification after implementation:

1. Issue a beta API key with the admin CLI.
2. Call `/health` without a key and confirm it works.
3. Call a protected beta route without a key and confirm it fails with the
   documented `401` shape.
4. Call a protected beta route with the issued key and confirm it works.
5. Exceed a configured limit and confirm the documented `429` shape.
6. Revoke the key.
7. Call the protected route again and confirm it fails with the documented
   `401` shape.
8. Inspect usage with the admin CLI and confirm raw keys and hashes are absent.

## Open Questions Before Acceptance

- What are the first beta default limits for `friend`, `partner`, `public`, and
  `internal` consumers, or should all issued keys start with explicit operator
  supplied limits?
- Should accepted beta route requests expose any public rate-limit headers, or
  should limits remain visible only through documented error behavior and
  operator usage commands?
- Should `GET /v1/status` remain disabled in beta mode, or should a future beta
  contract expose it as a public unauthenticated or authenticated readiness
  route?
- Should admin CLI output be JSON by default for automation, human-readable by
  default, or support both with an explicit format flag?

## Acceptance Criteria

This spec is complete when:

- A real partner/customer API key can be issued from the CLI.
- The raw key is shown once and never stored.
- Protected beta routes reject missing or invalid keys.
- Protected beta routes accept valid active keys.
- Revoked, disabled, expired, and disabled-consumer keys fail.
- Per-key daily limits are enforced atomically.
- Basic per-day usage is visible from the CLI.
- `/health` remains public.
- No public key-management endpoints exist.
- Migrations and reference data still create no real customer records or real
  API keys.
- `CONTRACTS.md`, OpenAPI, smoke docs, and `HISTORY.md` are updated in the same
  change as the runtime auth behavior.
- `cargo test`, `make test-db-postgres`, and `git diff --check` pass.

Until these criteria are met and the contract is updated, API-key
authentication is proposed behavior only.
