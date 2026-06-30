---
status: active
owner: iron-burrow
last_reviewed: 2026-06-25
agent_edit_policy: append_only
---

# HISTORY.md

Append-style project change log for notable Mother API contract,
implementation, and documentation changes.

## 2026-06-02

- Implemented optional `/v1/assets/{slug}` asset-detail enrichments via
  `include=priceStats,priceTrend,priceSeries`, backed by the existing
  price-indexer Query Layer client.
- Added partial enrichment failure reporting with `signals` null values and
  `enrichment_errors` while preserving the base asset-detail `200 OK`
  response when the asset exists.
- Updated `CONTRACTS.md` with the new asset-detail query parameters, optional
  signal fields, and enrichment error codes.
- Hardened the demo path with clearer asset-detail contract examples,
  enrichment failure observability, partial-failure regression coverage, and a
  Maria UI smoke checklist.

## 2026-06-03

- Drafted `SPEC-004-dis-polymarket-prediction-routes.md`: DIS-backed Mother API
  public/demo routes `GET /v1/predictions/fifa-world-cup/winner` and
  `GET /v1/predictions/fifa-world-cup/{country}` for the World Cup 2026 demo.
- Framed SPEC-004 as a sibling of SPEC-001: both are Mother API → DIS
  integrations (Aave V3 realized yield vs. Polymarket World Cup predictions),
  reusing the same DIS client foundation.
- Kept the spec in `draft`; no public endpoint is implemented yet and
  `CONTRACTS.md` is intentionally unchanged until the spec is accepted and the
  routes are built.
- Added the internal DIS prediction snapshot client slice: typed
  Polymarket snapshot DTOs, DIS config/state wiring, bounded retry/timeout
  behavior, and error classification without exposing public prediction
  routes.
- Implemented the SPEC-004 public/demo prediction routes:
  `GET /v1/predictions/fifa-world-cup/winner` and
  `GET /v1/predictions/fifa-world-cup/{country}`.
- Added app-level fake-DIS route tests for winner success, country
  normalization, unknown query parameter handling, sanitized public responses,
  decimal-string preservation, missing DIS config, and unsupported subjects.
- Updated `CONTRACTS.md` and `README.md` with the new prediction endpoint
  shapes and public error codes.
- Hardened SPEC-004 public error mapping with explicit provider
  unavailable/timeout coverage, sanitized DIS failure envelopes, missing-DIS
  graceful degradation, and the generic `internal_error` response constructor.
- Finalized the SPEC-004 judge-demo contract path with live Polymarket-implied
  wording, exact public error examples, local/dev curl smoke commands, and
  implemented-status spec language.

## 2026-06-04

- Wired `DIS_BASE_URL`, `DIS_REQUEST_TIMEOUT_MS`, and
  `DIS_RETRY_MAX_ATTEMPTS` into the Mother API Compose environment so the
  implemented prediction routes can reach DIS on `iron-burrow-net`.
- Added `checks.dis` to `/v1/status` as a config/client availability signal
  with `configured`, `not_configured`, and `invalid_config` states.
- Changed `checks.price_indexer` on `/v1/status` from a reserved
  `not_connected` placeholder to a config/client availability signal with
  `configured`, `not_configured`, and `invalid_config` states.

## 2026-06-06

- Forwarded `/v1/assets/{slug}?quoteCurrency=...` to the price-indexer latest
  price lookup so the base `price` block can return direct or derived prices in
  `USD`, `MXN`, `USDC`, or `BTC`.
- Preserved `USD` as the default and kept currency conversion and derivation
  entirely inside price-indexer.
- Rejected empty or unsupported asset-detail `quoteCurrency` values with
  `400 invalid_request` before calling price-indexer.

## 2026-06-17

- Accepted `SPEC-006-network-scoped-balances-v1.md` as the implementation
  target for Mother API network-scoped latest EVM balance snapshots.
- Aligned canonical network slugs, limits, grouping, concrete target mapping,
  response validation, error mapping, and pinned snapshot evidence with
  Bigwig 3.5.0.
- Kept `/v1/balances` and `/v1/balances/bulk` outside the binding public
  contract until implementation; this documentation-only change adds no
  endpoints, dependencies, migrations, or runtime behavior.
- Corrected SPEC-006 network eligibility so canonical catalog EVM networks,
  including `eth-mainnet`, are not constrained by a Mother-owned
  Base/Arbitrum allowlist; Bigwig remains the authority for internal
  operation-aware route resolution.
- Migrated active Base, Mantle, and Arbitrum catalog rows in place to
  `base-mainnet`, `mantle-mainnet`, and `arbitrum-mainnet`, preserving network
  IDs and existing asset mappings.
- Added the internal batch catalog resolver for native and ERC-20 balance
  targets, with precise unsupported-network, unsupported-asset, and
  unsupported-pair outcomes plus malformed-catalog rejection.
- Updated existing asset-detail chain maps to emit canonical EVM network
  slugs. No balance endpoint or Bigwig call was added in this slice.
- Added the authenticated, single-attempt Bigwig latest-balances client with
  typed request/evidence DTOs, strict known-code decoding, sanitized error
  classification, timeout handling, and `Retry-After` retention.
- Wired optional Bigwig URL/token configuration and a 30-second default
  timeout into application state and Compose, with only the Mother API service
  joining the client-facing Bigwig network.
- Added contract tests for the exact Mother-to-Bigwig request, complete,
  partial, and failed evidence envelopes, every documented request-wide error,
  malformed responses, redaction, and no-retry behavior. Public balance routes
  and orchestration remain unimplemented.
- Added the internal balance snapshot orchestration service with first-seen
  network grouping, complete preflight catalog planning, defensive target
  deduplication, unsupported-pair skips, and concurrent per-network Bigwig
  calls.
- Added strict Bigwig response validation for catalog chain IDs, cardinality,
  account-target correlations, ordering, status consistency, and pinned
  evidence, while mapping request-wide and item failures to sanitized internal
  balance outcomes.
- Preserved caller account and requested asset order for the later quote and
  response-shaping slice. Public balance routes and `CONTRACTS.md` remain
  unchanged.
- Added strict balance quote enrichment through the Price Indexer batch
  endpoint, preserving available, unavailable, unsupported, provider-failure,
  and malformed-response distinctions without changing existing asset-list
  price behavior.
- Added arbitrary-length decimal-string balance conversion and quote
  multiplication with catalog scales, including malformed Bigwig raw-amount
  rejection before pinned evidence is exposed.
- Added internal single and bulk balance response assemblers with catalog
  metadata, exact values, per-account evidence, skips, sanitized errors,
  summaries, and SPEC-006 complete/partial/failed status aggregation. Public
  balance routes and `CONTRACTS.md` remain unchanged.

## 2026-06-18

- Implemented the final SPEC-006 slice with public `POST /v1/balances` and
  `POST /v1/balances/bulk` routes backed by the existing catalog, Bigwig,
  Price Indexer, orchestration, and response-assembly layers.
- Added JSON request extraction, latest-only validation, canonical network and
  asset admission, EVM address validation, duplicate detection, quote-currency
  normalization, and 50-account / 20-asset / 1,000-item limits.
- Preserved Bigwig and Price Indexer runtime degradation as sanitized
  item-level `200 OK` outcomes while mapping request-wide catalog and internal
  failures to stable public error envelopes.
- Added route-level coverage for complete single and bulk snapshots,
  validation failures, strict identifiers, unsupported pair skips, provider
  degradation, evidence, ordering, and address-case preservation.
- Added the two endpoints and their requests, responses, limits, status
  semantics, evidence guarantees, skips, and public error codes to
  `CONTRACTS.md`; SPEC-006 is now fully implemented.
- Deprecated the legacy FIFA / World Cup prediction endpoints:
  - `GET /v1/predictions/fifa-world-cup/winner`
  - `GET /v1/predictions/fifa-world-cup/{country}`
- Retained the routes temporarily for compatibility while marking responses
  with deprecation metadata.
- Recorded that the legacy demo surface is outside the COTO-focused Mother API
  direction and is scheduled for removal in `v0.2.0`, with no replacement
  currently promised.

## 2026-06-25

- Replaced the public asset-detail `chain_maps` response field with
  `asset_network_maps`, flattening each entry to expose `network_slug`,
  `network_name`, `caip2`, `is_native`, and `address`.
- Added balance request rejection for reserved network alias fields
  `chain`, `chain_id`, and `chain_slug` while preserving tolerance for
  unrelated future JSON fields.
- Updated the public contract, README, and active/draft specs to keep
  canonical Mother API network identity aligned on `network_slug`.
- Accepted SPEC-007 as the implementation target for the future public
  ERC-20 transfer search endpoint, binding it to Bigwig 3.5.2's implemented
  internal transfer-extraction contract, DTOs, limits, timeout behavior, and
  error taxonomy.
- Kept `/v1/erc20-transfers/search` outside the public contract for this
  documentation-only slice; no Mother API route, runtime behavior, README
  endpoint, or `CONTRACTS.md` promise was added.
- Added PR 1 groundwork for ERC-20 transfer search: strict public DTOs,
  disabled-by-default config, startup limit validation against Bigwig's
  contract-address limit, and feature-gated OpenAPI generation without
  registering the public runtime route.

## 2026-06-29

- Completed PR 5 response shaping for `/v1/erc20-transfers/search`, converting
  Bigwig ERC-20 transfer evidence into the public customer-readable success
  response.
- Added catalog enrichment for known ERC-20 token contracts by
  `(network_slug, contract_address)`, including explicit contract filters and
  returned row tokens, while preserving unknown explicit contracts with
  nullable metadata.
- Preserved raw transfer amounts as strings, emitted decimal amounts only
  when catalog decimals are known, and trimmed trailing fractional zeros from
  transfer decimal strings.
- Propagated Bigwig's optional `truncated` success flag into
  `limits.truncated`, added the `200` OpenAPI response, and documented the
  implemented transfer-search contract in `CONTRACTS.md`.
- Completed PR 6 gated route wiring for `/v1/erc20-transfers/search`, keeping
  `ERC20_TRANSFERS_ENABLED` disabled by default and registering the route only
  when explicitly enabled.
- Preserved the safe disabled behavior as the normal unmatched-route `404`,
  while enabled routes without usable Bigwig extraction return
  `503 extraction_unavailable`.
- Added route-level coverage for enabled success through fake Bigwig,
  validation failures before dependency calls, asset-resolution failures,
  upstream failure mapping, route absence when disabled, and OpenAPI path
  gating.
- Hardened PR 7 public documentation for `/v1/erc20-transfers/search` with
  contract examples, OpenAPI examples, token-filter semantics, public limits,
  and error-code drift checks tied to DTO-shaped fixtures.
- Hardened balance request parsing for SPEC-008 PR 2 so unsupported JSON
  fields now return `400 unknown_field`, while reserved network aliases remain
  `400 invalid_request` and existing balance validation codes are preserved.
- Completed SPEC-008 PR 5 for the Beta balance surface, aligning
  `CONTRACTS.md`, OpenAPI-generated examples, and production smoke checks with
  the implemented single and bulk balance behavior.
- Documented Beta route-surface smoke coverage for active balance routes,
  known disabled endpoints returning `403 endpoint_disabled`, unknown routes
  remaining `404`, validation failures, skipped unsupported asset-network
  items, and sanitized item-level provider failures.
- Accepted `SPEC-008-balance-endpoint-beta-contract-hardening.md` after the
  consumer-facing docs and smoke checks were brought in line with the
  implementation, with `cargo test` passing for the release slice.
- Released the `v0.2.0` cleanup that removes the deprecated DIS-backed FIFA
  World Cup prediction endpoints:
  - `GET /v1/predictions/fifa-world-cup/winner`
  - `GET /v1/predictions/fifa-world-cup/{country}`
- Removed the active public route handlers, route registrations, contract
  docs, OpenAPI/public-route assertions, and prediction-only public error
  codes; these paths now follow normal unknown-route behavior.
- Archived SPEC-004 as historical demo memory. No replacement endpoint is
  currently promised; future prediction or intelligence endpoints require a
  new accepted spec.

## 2026-06-30

- Added the SPEC-009 Slice 1 database lifecycle command scaffold under the
  `mother-api` executable, including `serve`, `db migrate`,
  `db apply-reference`, and `db apply`.
- Kept database lifecycle work explicit: `serve` starts only the HTTP
  application, while the new `db` commands require `DATABASE_URL` and fail
  clearly until the later embedded-migration and reference-data slices land.
