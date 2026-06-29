---
status: draft
owner: iron-burrow
last_reviewed: 2026-06-29
agent_edit_policy: update_when_relevant
---

# SPEC-008 - Balance Endpoint Beta Contract Hardening

Draft implementation spec for tightening Mother API balance endpoints for the
private Beta surface.

This spec is not a public contract until accepted. Accepted
[SPEC-006](SPEC-006-network-scoped-balances-v1.md) remains the balance design
record, accepted [SPEC-007](SPEC-007-public-erc-20-transfer-search-v1.md)
remains the ERC-20 transfer-search design record, and
[CONTRACTS.md](../../CONTRACTS.md) remains authoritative for implemented public
callers.

## Summary

Mother API Beta should expose a deliberately small public surface:

- `GET /health`
- `POST /v1/balances`
- `POST /v1/balances/bulk`
- `POST /v1/erc20-transfers/search`, when its feature gate is enabled

The balance endpoints are already implemented and described by SPEC-006, but
they still need Beta-level public contract discipline: strict request parsing,
schema-ready HTTP DTOs, reusable examples, OpenAPI coverage, public error
examples, and route-surface tests.

This spec defines the remaining hardening delta. It does not add a new balance
capability or change balance orchestration ownership.

## Current State

- Runtime balance routes exist at `POST /v1/balances` and
  `POST /v1/balances/bulk`.
- Balance request DTOs exist in the HTTP adapter layer.
- Balance response shaping exists, but the response schema must be made safe
  as a public OpenAPI source instead of deriving public schemas from
  application or Bigwig internals.
- Current balance parsing ignores some unknown fields. Beta must reject unknown
  public JSON fields.
- Current `CONTRACTS.md` documents a larger Alpha surface. Any runtime or
  contract behavior change made while implementing this spec must update
  `CONTRACTS.md` in the same PR.

## Goals

- Make both balance endpoints ready for private Beta.
- Keep the Beta public route surface small, explicit, and testable.
- Reject unknown balance request fields with stable public errors.
- Keep `network_slug` as the only public network identity field.
- Prevent `chain`, `chain_id`, `chain_slug`, Bigwig route IDs, providers,
  pricing internals, and application-only planning fields from entering the
  public balance contract.
- Add balance OpenAPI paths, schemas, success examples, validation-error
  examples, and upstream-failure examples.
- Reuse balance examples across DTO tests, route tests, OpenAPI, contracts, and
  smoke checks where practical.

## Non-Goals

- No new balance pricing behavior.
- No new asset resolver behavior.
- No direct EVM JSON-RPC calls, indexing, protocol math, or cache scheduling in
  Mother API.
- No full balances application-port refactor.
- No API-key, billing, rate-limit, auth, or x402 implementation. API-key work
  belongs in SPEC-009 or a later auth-specific spec.
- No rename from `/v1/erc20-transfers/search` to `/v1/transfers/search`.

## Beta Route Surface

For Beta, only these public routes should be active:

| Method | Path | Notes |
| ------ | ---- | ----- |
| `GET` | `/health` | Dependency-free liveness. |
| `POST` | `/v1/balances` | Single-account balance convenience route. |
| `POST` | `/v1/balances/bulk` | Preferred batch balance route. |
| `POST` | `/v1/erc20-transfers/search` | Active only when its feature gate is enabled. |

Known non-Beta routes must not remain active in a Beta deployment, including:

- `GET /v1/status`
- `GET /v1/assets`
- `GET /v1/assets/resolve`
- `GET /v1/assets/{slug}`
- `GET /v1/assets/{slug}/signal/price-stats`
- `GET /v1/assets/{slug}/signal/price-trend`
- `GET /v1/search-engine`
- deprecated prediction/demo routes
- any incomplete or internal route not listed in the Beta table above

Known-but-disabled Beta routes should return a Mother API JSON error envelope:

```json
{
  "ok": false,
  "error": {
    "code": "endpoint_disabled",
    "message": "This endpoint is currently disabled for the Beta release."
  }
}
```

Use `403 Forbidden` for a known endpoint intentionally disabled in the Beta
deployment. Truly unknown routes remain normal `404 Not Found`.

## Balance DTOs and Examples

Balance public DTOs must be owned by the HTTP adapter boundary before they are
used as public OpenAPI schema sources.

Implementation requirements:

- Request DTOs live under `adapters/http/dto`.
- Response DTOs used for public schemas live under the HTTP adapter boundary
  or are otherwise clearly separated from internal application result types.
- Application result types remain internal orchestration data.
- Bigwig and Price Indexer DTOs remain internal adapter data.
- Public balance DTOs derive or expose the traits needed for JSON
  serialization, JSON parsing, and OpenAPI schema generation.
- Public examples live in a reusable balance examples module, preferably near
  the balance DTOs.

Reusable examples must cover:

- single-balance request and success response;
- bulk-balance request and success response;
- validation error response;
- item-level balance-provider failure response;
- unsupported asset-network skipped item;
- public limits.

Examples must not include `chain`, `chain_id`, `chain_slug`, Bigwig route IDs,
provider IDs, upstream URLs, authentication details, pricing internals, or
application-only planning fields.

## Balance Request Validation

Balance request parsing must be strict and tested for both balance routes.

Unknown JSON fields must return:

```txt
400 unknown_field
```

Strict unknown-field rejection applies to:

- top-level single-balance request fields;
- top-level bulk-balance request fields;
- `as_of`;
- `account`;
- `accounts[]`;
- `assets[]`.

Reserved network alias fields must be rejected anywhere they could imply public
network identity:

```txt
chain
chain_id
chain_slug
```

These fields must not be accepted as aliases for `network_slug`. Unless a
future contract update chooses a narrower code, reserved alias rejection remains
a public request error under the existing `invalid_request` family.

Validation must also cover:

- malformed JSON;
- missing or non-JSON `Content-Type`;
- missing required fields;
- wrong field types;
- unsupported `as_of.kind`;
- empty account or asset arrays;
- invalid account addresses;
- duplicate network-scoped accounts;
- duplicate asset slugs;
- invalid, legacy, non-EVM, or unsupported `network_slug` values;
- unknown or unsupported `asset_slug` values;
- unsupported quote currency;
- too many accounts, assets, or account-asset resolution items.

The accepted SPEC-006 public limits remain:

| Limit | Maximum |
| ----- | ------- |
| Accounts | 50 |
| Assets | 20 |
| Account-asset resolution items | 1,000 |

## Balance Error Behavior

Balance errors must use the public Mother API error envelope when the whole
request fails:

```json
{
  "ok": false,
  "error": {
    "code": "invalid_request",
    "message": "Request parameters are invalid."
  }
}
```

Request-wide errors must document:

- HTTP status;
- stable `error.code`;
- stable meaning;
- example response;
- whether the failure is request-side, dependency-side, or internal.

Bigwig and Price Indexer runtime failures for supported balance items remain
item-level balance response errors as specified by SPEC-006. They should be
covered by success-response examples with `status: "partial"` or
`status: "failed"` rather than incorrectly promoted to request-wide HTTP
errors.

Public errors and item-level errors must not expose Bigwig-only, provider-only,
or pricing-internal taxonomy unless that taxonomy is explicitly part of the
Mother API public contract.

## OpenAPI Requirements

OpenAPI for the Beta deployment must match the Beta public surface.

It must include:

- `GET /health`, if liveness routes are represented in OpenAPI;
- `POST /v1/balances`;
- `POST /v1/balances/bulk`;
- `POST /v1/erc20-transfers/search`, only when enabled.

Each balance path must include:

- request schema;
- success response schema;
- public error response schema;
- at least one success example;
- at least one validation-error example;
- at least one item-level upstream/provider failure example;
- documented public limits;
- documented quote currency behavior;
- documented `network_slug` behavior.

OpenAPI must not expose non-Beta routes as active Beta paths. If disabled
routes are documented at all, they must be explicitly documented as disabled
with `endpoint_disabled`.

## Implementation PR Split

### PR 1 - Balance DTO Schemas and Examples

Goal: make balance HTTP DTOs and examples safe as public schema sources.

Scope:

- Add or move public balance response DTOs to the HTTP adapter boundary.
- Add schema derivations for balance requests, responses, and shared payloads.
- Add reusable examples for single, bulk, validation, skipped, and item-level
  failure scenarios.
- Add DTO parse and serialization tests.

### PR 2 - Strict Balance Validation

Goal: make balance request failures Beta-contract-safe.

Scope:

- Reject unknown fields with `400 unknown_field`.
- Reject reserved network aliases.
- Preserve existing public validation codes where they already match
  `CONTRACTS.md`.
- Keep malformed JSON, missing required fields, wrong types, and invalid
  `Content-Type` mapped to public request errors.
- Add route and DTO tests for all validation branches.

### PR 3 - Balance OpenAPI Coverage

Goal: expose balance endpoints accurately in generated OpenAPI.

Scope:

- Add both balance paths.
- Add schemas and examples from the public DTO/example modules.
- Add tests for path presence, response status coverage, public fields,
  example validity, and absence of hidden or disabled routes.

### PR 4 - Beta Route Surface Gate

Goal: make runtime route availability match the Beta surface.

Scope:

- Keep `/health`, `/v1/balances`, and `/v1/balances/bulk` active.
- Keep `/v1/erc20-transfers/search` active only when enabled.
- Disable known non-Beta routes for Beta deployments.
- Add `403 endpoint_disabled` JSON behavior for known disabled routes.
- Preserve normal `404` behavior for truly unknown routes.

### PR 5 - Contracts, Docs, and Smoke Checks

Goal: make the implemented Beta balance surface usable by consumers.

Scope:

- Update `CONTRACTS.md` for every public behavior changed by the implementation.
- Update OpenAPI artifacts or generation as the repo expects.
- Add or update smoke payloads for single and bulk balances.
- Add success, validation-error, disabled-route, and item-level failure
  examples.
- Add `HISTORY.md` or release notes if the repo uses them for the change.

## Test Plan

Implementation must add or update tests for:

- single balance request parses;
- bulk balance request parses;
- public balance responses serialize to expected JSON;
- unknown fields reject with `unknown_field`;
- reserved aliases reject as public request errors;
- malformed JSON and invalid `Content-Type` reject consistently;
- invalid network, unsupported network, invalid address, unsupported quote
  currency, duplicate account, duplicate asset, too many accounts, too many
  assets, and too many resolution items;
- unsupported asset-network pairs are skipped without provider calls;
- Bigwig and Price Indexer runtime failures remain sanitized item-level balance
  errors;
- OpenAPI contains both balance paths and expected public schemas;
- OpenAPI examples deserialize or serialize as appropriate;
- OpenAPI does not expose hidden non-Beta routes as active paths;
- Beta route allowlist is enforced;
- known disabled endpoints return `403 endpoint_disabled`;
- unknown routes remain `404`.

Before calling balances Beta-ready, `cargo test` must pass.

## Beta Exit Criteria

Balances are Beta-ready when:

- `POST /v1/balances` is publicly documented, tested, and OpenAPI-covered.
- `POST /v1/balances/bulk` is publicly documented, tested, and
  OpenAPI-covered.
- Public DTOs are safe schema sources and do not derive public schemas from
  Bigwig or application internals.
- Unknown balance request fields are rejected.
- Public examples parse and serialize.
- Public validation and item-level failure behavior are documented and tested.
- Public limits are documented and tested.
- Runtime route surface and OpenAPI route surface agree for Beta.
- Known disabled routes return `403 endpoint_disabled`.
- `CONTRACTS.md` is updated in the same PR as any public behavior change.
- `cargo test` passes.

## Deferred Follow-Up

After SPEC-008 balance hardening and SPEC-007 transfer-search readiness are
clean, continue in SPEC-009 or a later auth-specific spec with:

- API-key protection for Beta endpoints;
- client identity logging;
- full Beta auth tests;
- optional deeper balances application-port refactor;
- optional route/module cleanup.
