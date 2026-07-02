---
status: accepted
owner: iron-burrow
last_reviewed: 2026-07-02
agent_edit_policy: update_when_relevant
---

# SPEC-011 - Public API Beta v0.2 Surface v1

Draft release-readiness specification for the private Beta v0.2 Mother API
surface.

This spec does not create new public endpoints or override the binding public
contract. It collects the already accepted Beta slices into one release-facing
artifact so maintainers can review whether the v0.2 surface is coherent,
documented, protected, observable, and smoke-testable.

Named customer relationship details are intentionally out of scope for this
repository. This document uses "private beta client" and "early API consumer"
language only.

## Authoritative Sources

- [CONTRACTS.md](../../CONTRACTS.md) is authoritative for exact public request
  bodies, response bodies, validation behavior, limits, and error envelopes.
- [README.md](../../README.md) is the high-level public Beta navigation guide.
- [SPEC-006](SPEC-006-network-scoped-balances-v1.md) is the accepted balance
  design record.
- [SPEC-007](SPEC-007-public-erc-20-transfer-search-v1.md) is the accepted
  ERC-20 transfer search design record.
- [SPEC-008](SPEC-008-balance-endpoint-beta-contract-hardening.md) is the
  accepted Beta balance hardening record.
- [SPEC-010](SPEC-010-beta-api-key-access-service.md) is the accepted Beta
  API-key access-service record.
- [docs/smoke-tests.md](../smoke-tests.md) is the production smoke-test
  runbook for the Beta release surface.

When this spec conflicts with `CONTRACTS.md`, `CONTRACTS.md` wins and this
draft must be corrected before acceptance.

## Goal

Beta v0.2 exposes a deliberately small, API-key-protected public surface that
is stable enough for early private beta clients:

| Method | Path | Auth | Runtime availability |
| ------ | ---- | ---- | -------------------- |
| `GET` | `/health` | Public | Always registered. |
| `POST` | `/v1/balances` | API key in Beta mode | Registered in Beta mode. |
| `POST` | `/v1/balances/bulk` | API key in Beta mode | Registered in Beta mode. |
| `POST` | `/v1/erc20-transfers/search` | API key in Beta mode | Registered only when `ERC20_TRANSFERS_ENABLED=true`. |

The original release brief focused on bulk balances and ERC-20 transfer
search. The implemented and documented Beta surface also includes the
single-account `/v1/balances` convenience route. SPEC-011 accepts that route as
part of v0.2 rather than creating a contract mismatch.

## Non-Goals

- No old FIFA, prediction, admin, explorer, account, tracked-token, price,
  billing, x402, or public API-key management routes.
- No public auth platform, OAuth, JWT, customer portal, or self-service key
  management.
- No in-process response caching.
- No Mother-owned price indexing, event indexing, holder indexing, direct EVM
  JSON-RPC calls, protocol math, or Bigwig route/provider exposure.
- No alternate or renamed transfer-search route in v0.2.
- No landing page implementation in this repository unless a separate accepted
  document assigns that ownership to Mother API.

## Runtime Route Surface

Production private Beta deployments should run with:

```text
PUBLIC_API_SURFACE=beta
```

In this mode:

- `/health` remains public.
- Protected Beta `/v1/*` routes require
  `Authorization: Bearer <issued_beta_api_key>`.
- Known Alpha-only routes return `403 endpoint_disabled`.
- Truly unknown routes remain normal `404` responses.
- The ERC-20 transfer route is absent unless `ERC20_TRANSFERS_ENABLED=true`.

Alpha compatibility mode may still expose the broader Production Alpha 1
surface, but that is not the private Beta v0.2 customer surface.

## Endpoint Contract Summary

### `POST /v1/balances/bulk`

Binding contract: `CONTRACTS.md`, accepted design: SPEC-006 and SPEC-008.

Request DTO:

| Field | Type | Notes |
| ----- | ---- | ----- |
| `as_of.kind` | string | Must be `"latest"`. Historical balance snapshots are deferred. |
| `accounts[]` | array | One to 50 explicit network-scoped accounts. |
| `accounts[].network_slug` | string | Canonical Mother API network slug. Do not accept `chain`, `chain_id`, or `chain_slug` as public aliases. |
| `accounts[].address` | string | EVM account address. |
| `accounts[].client_ref` | string or null | Opaque caller reference, echoed unchanged. |
| `quote_currency` | string | `USD`, `MXN`, `USDC`, or `BTC`. |
| `assets[]` | array | One to 20 exact canonical asset slugs. |
| `assets[].asset_slug` | string | Network-agnostic global asset slug. |

Response DTO:

| Field | Type | Notes |
| ----- | ---- | ----- |
| `ok` | bool | `true` for accepted balance responses. |
| `type` | string | Always `"balances_bulk"`. |
| `status` | string | `complete`, `partial`, or `failed`. |
| `as_of.kind` | string | Always `"latest"`. |
| `quote_currency` | string | Normalized quote currency. |
| `summary` | object | Requested counts, returned positions, skipped items, failed items. |
| `accounts[]` | array | Per-account balance response with evidence, positions, skipped items, and item errors. |
| `errors[]` | array | Reserved top-level diagnostics; currently empty on success. |

Supported networks:

- Canonical EVM `network_slug` values that are present in Mother API catalog,
  have active asset mappings for the requested assets, and are callable through
  Bigwig latest-balance evidence.
- Examples include `eth-mainnet`, `base-mainnet`, `mantle-mainnet`, and
  `arbitrum-mainnet` when catalog and Bigwig support are present.
- Unknown, non-EVM, legacy, or non-canonical slugs return
  `400 unsupported_network`.

Supported assets:

- Exact canonical `asset_slug` values from Mother API's global asset catalog.
- Unsupported asset-network pairs are represented as `skipped[]` entries with
  reason `asset_not_supported_on_network`.
- Unknown or non-canonical asset slugs return `400 unsupported_asset`.

Limits:

| Limit | Maximum |
| ----- | ------- |
| Accounts | 50 |
| Assets | 20 |
| Account-asset resolution items | 1,000 |

Upstream behavior:

- Bigwig and Price Indexer runtime failures for supported balance items remain
  item-level errors inside `200 OK` responses.
- Request-wide catalog unavailability returns `503 asset_network_map_unavailable`.
- Mother API must not expose Bigwig routes, providers, URLs, chain IDs, or
  authentication details.

Examples:

- Request, response, skipped-item, validation-error, request-too-large, and
  item-level provider-failure examples live in `CONTRACTS.md` and generated
  OpenAPI. SPEC-011 intentionally does not duplicate those JSON bodies.

### `POST /v1/erc20-transfers/search`

Binding contract: `CONTRACTS.md`, accepted design: SPEC-007.

Request DTO:

| Field | Type | Notes |
| ----- | ---- | ----- |
| `network_slug` | string | Required. Currently only `eth-mainnet`. |
| `address` | string | Watched wallet address, not the token contract address. |
| `direction` | string | `any`, `from`, or `to`. |
| `tokens.asset_slugs` | array | Optional exact catalog asset slugs that must resolve to ERC-20 contracts on `network_slug`. |
| `tokens.contract_addresses` | array | Optional explicit ERC-20 contract addresses. Unknown contracts are valid filters. |
| `window` | object | Exactly one block, timestamp, or lookback window shape. |

Response DTO:

| Field | Type | Notes |
| ----- | ---- | ----- |
| `ok` | bool | `true` for successful transfer search responses. |
| `type` | string | Always `"erc20_transfer_search"`. |
| `network_slug` | string | Accepted network slug. |
| `address` | string | Normalized watched wallet address. |
| `direction` | string | Accepted direction. |
| `window` | object | Accepted bounded window. |
| `token_filters` | object | Requested filters and resolved concrete contract set. |
| `transfers[]` | array | Transfer rows returned by Bigwig and shaped by Mother API. |
| `limits` | object | `max_rows` and `truncated`. |

Supported networks:

- `eth-mainnet` only for v0.2 transfer search.

Supported token filters:

- `tokens` may be omitted, `null`, `{}`, or empty for unfiltered ERC-20
  transfer search inside a bounded window.
- `tokens.asset_slugs[]` must resolve to ERC-20 contracts on `network_slug`.
- Native assets are rejected; Mother API must not silently convert native ETH
  into WETH. Callers must request `wrapped-ether` or provide a WETH contract
  address.
- `tokens.contract_addresses[]` must be concrete `0x` 20-byte EVM addresses.
  Unknown explicit contracts remain valid filters and return nullable catalog
  metadata.

Limits:

| Limit | Maximum |
| ----- | ------- |
| Block window on `eth-mainnet` | 5,000 inclusive blocks |
| Lookback window | 86,400 seconds |
| Unique token filters after resolution and deduplication | 20 |
| Returned rows | 5,000 |

Timestamp windows are supported by the current public DTO and contract as an
alternative window shape. `CONTRACTS.md` documents `window_too_large` for
block, timestamp, and lookback windows; before accepting this release spec,
maintainers should confirm whether the timestamp-window maximum needs an
explicit published value.

Upstream behavior:

- Bigwig owns extraction, provider access, finality, chunking, row limits,
  timeout behavior, and provider-specific failures.
- Mother API owns validation, asset-contract resolution, response shaping, and
  public error mapping.
- `limits.truncated: true` is a valid success response capped by the upstream
  row limit.

Examples:

- Unfiltered, asset-slug, contract-address, mixed-filter, native-asset
  rejection, unknown-slug rejection, too-many-filter, truncated, and upstream
  failure examples live in `CONTRACTS.md` and generated OpenAPI. SPEC-011
  intentionally does not duplicate those JSON bodies.

## API-Key Access

Binding contract: `CONTRACTS.md`, accepted design: SPEC-010.

Private Beta `/v1/*` routes require:

```http
Authorization: Bearer <api_key>
```

Minimum accepted behavior:

- Operators issue keys through the Mother API admin CLI.
- Each private beta client receives a distinct key.
- Requests are associated with an `ApiKeyPrincipal` containing key and consumer
  identity.
- Keys can be revoked.
- Per-key request limits are enforced.
- Daily accepted, rate-limited, successful, client-error, and server-error
  counters are tracked.
- Raw keys are printed only once at issuance and are never stored, logged,
  returned in errors, or shown by list/usage commands.

This access layer is not a general identity platform and does not expose public
key-management endpoints.

## Error Behavior

All public request-wide errors use the Mother API JSON envelope:

```json
{
  "ok": false,
  "error": {
    "code": "invalid_request",
    "message": "Request parameters are invalid."
  }
}
```

Stable error behavior required for v0.2:

| Case | Expected behavior |
| ---- | ----------------- |
| Missing, malformed, unsupported, unknown, disabled, revoked, expired, or disabled-consumer API key | `401 unauthorized` |
| Valid key exceeds configured request limits | `429 rate_limited` |
| API-key storage unavailable during auth | `503 database_unavailable` |
| Known Alpha-only route in Beta mode | `403 endpoint_disabled` |
| Balance unsupported or invalid network slug | `400 unsupported_network` |
| Balance unknown asset slug | `400 unsupported_asset` |
| Balance too many accounts/assets/items | `400 request_too_large` |
| Transfer missing network slug | `400 missing_network_slug` |
| Transfer unsupported network | `404 unsupported_network` |
| Transfer unknown asset slug | `404 asset_not_found` |
| Transfer native or non-ERC-20 asset filter | `422 asset_not_erc20_on_network` |
| Transfer invalid contract address | `400 invalid_contract_address` |
| Transfer too many unique token filters | `422 too_many_token_filters` |
| Transfer block, timestamp, or lookback window too large | `422 window_too_large` |
| Transfer extraction unavailable | `503 extraction_unavailable` |
| Transfer upstream provider error | `502 upstream_provider_error` |
| Transfer upstream provider timeout | `504 upstream_provider_timeout` |
| Transfer overall extraction timeout | `504 extraction_timeout` |

Unknown slugs, invalid filters, native asset misuse, and unsupported networks
must fail explicitly. They must not silently return successful empty result
sets.

## Observability

Minimum v0.2 visibility requirements:

- Protected-route usage is attributable to API key and consumer identity
  without exposing raw secrets.
- Usage counters record accepted requests, rate-limited requests, successful
  responses, client errors, server errors, and last-used time.
- Logs may include route, status code, request duration, `consumer_slug`,
  `api_key_id`, `key_prefix`, and limit outcome.
- Transfer and balance failure logs may include sanitized upstream error class.
- Logs must not include raw API keys, key hashes, full `Authorization` headers,
  full request bodies for auth debugging, provider secrets, or raw internal
  diagnostics.

## Documentation Requirements

Before this spec can be accepted, these docs must be aligned:

- `CONTRACTS.md` documents the binding Beta route surface, auth behavior,
  limits, examples, and error catalogue.
- README stays brief and navigational.
- Generated OpenAPI includes protected-route schemas, examples, and
  `BetaApiKeyAuth`.
- `docs/smoke-tests.md` includes production Beta balance checks, API-key checks,
  and optional transfer-search checks.
- `HISTORY.md` records the implemented release slice.

Private beta users should be pointed to a landing page and formal API docs
instead of raw curl commands in chat. Curl examples remain appropriate inside
the repository docs and smoke runbooks.

## Implementation PR Breakdown

The underlying runtime slices are already represented by accepted specs. If
SPEC-011 is used to organize review or release PRs, split work this way:

1. Contract reconciliation
   - Verify `CONTRACTS.md` and README match the intended v0.2 surface.
   - Confirm old FIFA/demo routes are removed from active contract and runtime.
   - Confirm no Alpha-only route is accidentally exposed in Beta mode.

2. Balance Beta verification
   - Verify `/v1/balances` and `/v1/balances/bulk` DTOs, limits, examples,
     OpenAPI paths, and validation errors match SPEC-006/SPEC-008.
   - Confirm balance upstream degradation remains item-level where contracted.

3. ERC-20 transfer gate verification
   - Verify `/v1/erc20-transfers/search` remains feature-gated.
   - Verify route, DTOs, token filters, limits, explicit errors, OpenAPI
     examples, and Bigwig failure mapping match SPEC-007 and `CONTRACTS.md`.

4. API-key access verification
   - Verify SPEC-010 operator CLI, auth middleware, per-key limits, revocation,
     usage counters, OpenAPI security, and non-enumerating errors.

5. Release docs and smoke checks
   - Verify `docs/smoke-tests.md` and `scripts/smoke/beta-auth.sh` cover the
     release gate.
   - Add a `HISTORY.md` entry only when runtime or contract behavior changes.

## Open Questions

- Should the transfer timestamp-window maximum be explicitly published in
  `CONTRACTS.md` before accepting SPEC-011, or is the current
  `window_too_large` behavior sufficient for v0.2?
- Should `ERC20_TRANSFERS_ENABLED=true` be part of the first external private
  beta deployment, or should the route remain documented but disabled until
  Bigwig extraction readiness is proven in the target environment?
- What default per-key limits should operators use for the first private beta
  clients, beyond the SPEC-010 CLI defaults of 60 requests per minute and 5,000
  requests per day?
- Should any public rate-limit headers be added in a later release, or should
  v0.2 keep limits visible only through `429 rate_limited` and operator usage
  commands?

## Blockers

No product or architecture blocker is known for making this draft
review-ready.

External transfer-search enablement remains blocked in any target environment
until:

- Bigwig Hub extraction is enabled and reachable;
- Mother API has `INFRA_GATEWAY_URL`, `INFRA_GATEWAY_TOKEN`, and a suitable
  `BIGWIG_REQUEST_TIMEOUT_MS`;
- production transfer smoke checks pass without `extraction_unavailable`,
  `upstream_provider_timeout`, or `extraction_timeout` on the valid USDC smoke
  payload.

SPEC-011 acceptance should wait until the timestamp-window open question is
resolved or explicitly deferred.

## Smoke Test Checklist

Release gate:

- Run `cargo fmt --check`.
- Run `cargo test`.
- Run `make test-db-postgres`.
- Run `make smoke-db-lifecycle`.
- Run `make smoke-beta-auth`.

Production Beta balance gate:

- Confirm deployment uses `PUBLIC_API_SURFACE=beta`.
- Confirm `/health` returns `200` without an API key.
- Confirm known Alpha-only routes return `403 endpoint_disabled`.
- Confirm unknown routes return `404`.
- Confirm protected routes without a key return `401 unauthorized`.
- Issue a throwaway API key and confirm a protected route reaches route
  validation.
- Exceed a tiny configured limit and confirm `429 rate_limited`.
- Revoke the throwaway key and confirm it returns `401 unauthorized`.
- Inspect usage and confirm raw keys and hashes are absent.
- Confirm `/v1/balances` returns the contracted single-balance shape.
- Confirm `/v1/balances/bulk` returns the contracted bulk-balance shape.
- Confirm balance unknown fields return `400 unknown_field`.
- Confirm reserved balance network aliases return `400 invalid_request`.

Optional ERC-20 transfer gate:

- Confirm route registration only when `ERC20_TRANSFERS_ENABLED=true`.
- Confirm unfiltered bounded search returns the contracted `200` shape.
- Confirm USDC asset-slug search resolves the Ethereum mainnet USDC contract.
- Confirm explicit USDC contract search is accepted and normalized.
- Confirm mixed filters deduplicate to the concrete contract set.
- Confirm native `ethereum` asset filtering returns
  `422 asset_not_erc20_on_network`.
- Confirm an unknown asset slug returns `404 asset_not_found`.
- Confirm too-large block windows return `422 window_too_large`.
- Confirm too many token filters return `422 too_many_token_filters`.
- Confirm provider or extraction failures map only to documented public errors.

## Acceptance Criteria

SPEC-011 can be accepted when:

- The v0.2 public Beta route surface is exactly the one documented here and in
  `CONTRACTS.md`.
- API-key protection is active for Beta `/v1/*` routes.
- Request usage is attributable by API key and consumer without logging
  secrets.
- Balance and transfer limits are enforced and documented.
- Invalid networks, unknown slugs, native asset misuse, invalid contract
  addresses, token-filter limits, and range-limit violations fail explicitly.
- OpenAPI includes request, response, auth, and error examples for the Beta
  surface.
- Smoke checks cover happy paths, auth failures, validation failures, disabled
  routes, and upstream failure classifications.
- Known limitations and release gates are documented without overselling Mother
  API capabilities.
