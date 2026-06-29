# SPEC-008 — Balance Endpoint Beta Contract Hardening

Status: Draft
Owner: Mother API
Target: Beta release
Scope: Public balance endpoints, public Beta route surface, OpenAPI, DTOs, errors, examples

## 1. Context

Mother API Beta will expose a deliberately slim public API surface.

The first public capabilities are:

1. Balances
2. ERC-20 transfer search

Both capabilities already exist in runtime code. However, the ERC-20 transfer search endpoint is closer to the desired Beta standard: explicit HTTP DTOs, OpenAPI coverage, public examples, feature gating, and clearer adapter boundaries.

The balance endpoints are functionally solid, but they lag behind in public contract discipline. This spec hardens balances for Beta without performing a full architecture rewrite.

## 2. Goals

Make the balance endpoints Beta-ready by ensuring:

* Public request DTOs are explicit and stable.
* Public response DTOs are explicit and stable.
* Public errors are documented and tested.
* OpenAPI includes balance paths, schemas, success examples, and error examples.
* Examples are reusable from tests/docs/OpenAPI where practical.
* Runtime route behavior and OpenAPI agree.
* No internal Bigwig, price-indexer, catalog, or application-only fields leak into the public contract.
* The Beta route surface is honest, slim, and explicit.

## 3. Non-goals

This spec does not require:

* Full balances application-port refactor.
* Full removal of concrete Bigwig or Price Indexer clients from the balance application service.
* Large module reorganization.
* New pricing behavior.
* New asset resolver behavior.
* API-key implementation.

API-key protection is required for the final Beta release, but it is intentionally deferred until balances and ERC-20 transfer search are clean and contract-ready. API-key work should remain in SPEC-009 or a follow-up auth-specific spec/PR.

## 4. Beta Public Surface Decision

For Beta, the public runtime surface should be intentionally small.

### Keep public

* `GET /health`
* `POST /v1/balances`
* `POST /v1/balances/bulk`
* `POST /v1/erc20-transfers/search`

### Disable for Beta

The following should not be available as active public Beta endpoints:

* `GET /v1/status`
* `GET /v1/assets`
* `GET /v1/assets/resolve`
* `GET /v1/assets/{slug}`
* `GET /v1/assets/{slug}/signal/price-stats`
* `GET /v1/assets/{slug}/signal/price-trend`
* Deprecated FIFA prediction routes
* Any old search-engine/demo route
* Any incomplete or internal route not explicitly listed above

## 5. Endpoint Path Decisions

### Balances

Both balance endpoints must be ready for Beta:

* `POST /v1/balances`
* `POST /v1/balances/bulk`

`POST /v1/balances` is the single-balance convenience endpoint.

`POST /v1/balances/bulk` is the preferred batch endpoint.

Both must have stable DTOs, OpenAPI coverage, examples, public errors, and tests.

### ERC-20 Transfer Search

The Beta transfer route remains:

* `POST /v1/erc20-transfers/search`

Do not rename it to `/v1/transfers/search` for Beta.

`/v1/transfers/search` is intentionally reserved for a future, more generic transfer-search API that may include native transfers, non-ERC-20 events, cross-chain abstractions, or richer transfer types.

## 6. Disabled Endpoint Behavior

Mother API should be honest.

Known-but-disabled endpoints should not silently behave like they never existed.

For Beta-disabled routes, prefer a JSON Mother API error envelope instead of a bare `404`.

Example public error:

```json
{
  "error": {
    "code": "endpoint_disabled",
    "message": "This endpoint is currently disabled for the Beta release."
  }
}
```

Recommended HTTP status:

* `403 Forbidden` if the endpoint exists but is intentionally unavailable in this deployment.
* `404 Not Found` only for truly unknown routes that Mother API does not recognize.

Disabled endpoint responses must not expose internal implementation details.

## 7. Balance Public DTO Requirements

Balance HTTP DTOs must be owned by the HTTP adapter layer.

Expected shape:

* Public request DTOs live under `adapters/http/dto`.
* Public response DTOs live under `adapters/http/dto`.
* Application result types remain internal.
* Bigwig DTOs remain internal to the Bigwig adapter.
* OpenAPI schemas derive from public HTTP DTOs, not application internals.

The balance DTOs should support:

* `Serialize` where used for examples/responses.
* `Deserialize` for requests.
* `ToSchema` or equivalent OpenAPI derivation.
* Stable field names.
* Explicit public examples.

## 8. Balance Request Parsing and Validation

Balance request parsing must be explicit and tested.

The implementation must decide and enforce strict behavior for:

* Unknown top-level fields.
* Unknown account/subject fields.
* Unknown asset fields.
* Reserved aliases such as `chain`, `chain_id`, or `chain_slug`.
* Missing required fields.
* Empty arrays.
* Invalid network slugs.
* Invalid addresses.
* Too many subjects/accounts.
* Too many assets.
* Unsupported quote currency.
* Malformed JSON.
* Missing or invalid `Content-Type`.

For Beta, prefer strict parsing. Unknown fields should produce a public `unknown_field`-style error rather than being silently ignored.

## 9. Balance Public Error Requirements

Balance errors must use the public Mother API error envelope.

The public contract must document:

* Error code.
* HTTP status.
* Meaning.
* Example response.
* Whether the error is request-side, upstream-side, or internal.

At minimum, balance error tests should cover:

* Malformed JSON.
* Invalid content type.
* Unknown field.
* Reserved alias.
* Invalid network.
* Unsupported network.
* Invalid address.
* Too many subjects.
* Too many assets.
* Upstream unavailable.
* Upstream timeout.
* Provider failure.
* Malformed upstream success response, if applicable.

Internal Bigwig or provider errors must be mapped to stable Mother API public errors.

No public error should expose Bigwig-only taxonomy unless that taxonomy is intentionally part of the Mother API contract.

## 10. OpenAPI Requirements

OpenAPI must include both balance endpoints:

* `POST /v1/balances`
* `POST /v1/balances/bulk`

Each path must include:

* Request schema.
* Success response schema.
* Public error response schema.
* At least one success example.
* At least one validation error example.
* At least one upstream/provider error example if applicable.
* Documented limits.
* Documented quote currency behavior.
* Documented network slug behavior.

OpenAPI must not include disabled Beta routes unless they are explicitly documented as disabled.

The OpenAPI path set for Beta should match the accepted Beta surface.

## 11. Example Requirements

Balance examples should be centralized and reusable.

Prefer a structure such as:

* `adapters/http/dto/balances/examples.rs`

Examples should feed:

* DTO serialization tests.
* DTO deserialization tests.
* OpenAPI examples.
* CONTRACTS.md examples where practical.
* Smoke test payloads where practical.

Examples must not include deprecated fields such as `chain`, `chain_id`, or `chain_slug`.

Examples must not include internal fields such as:

* Bigwig route IDs.
* Provider IDs.
* Internal evidence-only fields not part of the public contract.
* Pricing internals.
* Application-only planning fields.

## 12. Testing Requirements

The balance hardening work must include tests for:

### DTO tests

* Single balance request parses.
* Bulk balance request parses.
* Public response serializes to expected JSON.
* Unknown fields are rejected.
* Reserved aliases are rejected.
* Deprecated fields do not appear in examples.

### Route tests

* `POST /v1/balances` is reachable.
* `POST /v1/balances/bulk` is reachable.
* Both return the expected public envelope.
* Public limit errors use stable codes/statuses.
* Malformed request errors use stable codes/statuses.

### OpenAPI tests

* OpenAPI contains both balance paths.
* OpenAPI schemas contain expected public fields.
* OpenAPI examples deserialize.
* OpenAPI does not expose hidden/disabled routes.
* OpenAPI path set matches the Beta public surface decision.

### Route surface tests

* `GET /health` is reachable.
* Balance endpoints are reachable.
* ERC-20 transfer search is reachable when enabled.
* Assets/resolver/status/prediction/demo endpoints are disabled for Beta.
* Disabled known endpoints return the chosen `endpoint_disabled` JSON error.
* Truly unknown routes can still return normal `404`.

## 13. Documentation Requirements

Update:

* `CONTRACTS.md`
* OpenAPI generation
* Smoke test runbook
* `HISTORY.md` or release notes if the repo uses it

Docs must include:

* Single balance curl example.
* Bulk balance curl example.
* Success response examples.
* Validation error examples.
* Disabled endpoint behavior.
* Public limits.
* Statement that API-key protection is planned for Beta but implemented in a later auth PR/spec.

## 14. Recommended PR Split

### PR 1 — Accept SPEC-009

Goal: record final Beta balance and route-surface decisions.

Scope:

* Accept both `POST /v1/balances` and `POST /v1/balances/bulk`.
* Keep transfer route as `/v1/erc20-transfers/search`.
* Keep `/health`.
* Disable non-Beta routes.
* Define disabled endpoint behavior.
* Defer API-key implementation.

Tests:

* None required beyond docs/spec checks.

### PR 2 — Balance Public DTOs and Examples

Goal: make balance HTTP DTOs explicit and reusable.

Scope:

* Add adapter-owned request/response DTOs.
* Add schema derivations.
* Add examples module.
* Add DTO parse/serialization tests.

Non-goals:

* No route-surface gating.
* No application-port refactor.
* No auth.

### PR 3 — Balance Validation and Error Discipline

Goal: make balance request failures public-contract-safe.

Scope:

* Strict unknown-field behavior.
* Reserved alias rejection.
* Public error mapping.
* Configured/fixed limit behavior.
* Malformed JSON/content-type behavior.

Tests:

* Route and DTO tests for all public validation errors.

### PR 4 — Balance OpenAPI Coverage

Goal: expose balances correctly in OpenAPI.

Scope:

* Add both balance paths.
* Add schemas.
* Add success/error examples.
* Add OpenAPI path/schema/example tests.

Tests:

* OpenAPI contains balance paths.
* Examples deserialize.
* Hidden routes are not documented.

### PR 5 — Beta Route Surface Gate

Goal: make runtime match the slim Beta surface.

Scope:

* Keep `/health`.
* Keep both balance endpoints.
* Keep ERC-20 transfer search when enabled.
* Disable assets/resolver/status/predictions/demo routes.
* Add `endpoint_disabled` error behavior for known disabled routes.

Tests:

* Public route allowlist.
* Disabled route JSON error.
* Unknown route still behaves as true unknown route.

### PR 6 — Balance Docs and Smoke Runbook

Goal: make balance endpoints usable by Beta consumers.

Scope:

* Update contracts.
* Add curl examples.
* Add smoke payloads.
* Add success/error examples.
* Add release notes.

Tests:

* Docs examples parse where practical.
* Smoke payloads match DTOs.

## 15. Beta Exit Criteria for Balances

Balances are Beta-ready when:

* `POST /v1/balances` is publicly documented, tested, and OpenAPI-covered.
* `POST /v1/balances/bulk` is publicly documented, tested, and OpenAPI-covered.
* Public DTOs are adapter-owned.
* Application/internal DTOs are not the public schema source.
* Bigwig DTOs do not leak into public responses or docs.
* Public examples parse and serialize.
* Public errors are documented and tested.
* Public limits are documented and tested.
* Runtime route surface and OpenAPI route surface agree.
* Disabled known endpoints return the chosen honest JSON error.
* `cargo test` passes.

## 16. Deferred Follow-up

After balance and transfer endpoints are clean and contract-ready, continue with:

* API-key protection for Beta endpoints.
* Full Beta auth tests.
* Client identity logging.
* Optional deeper balances application-port refactor.
* Optional route/module architecture cleanup.
