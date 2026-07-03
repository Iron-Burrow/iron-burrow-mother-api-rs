---
status: draft
owner: iron-burrow
last_reviewed: 2026-07-03
agent_edit_policy: update_when_relevant
---

# SPEC-012 - Balance Endpoint v0.3 Explicit Token Selectors and Historical Balances

Draft replacement spec for the private Beta balance endpoints.

This document does not override [CONTRACTS.md](../../CONTRACTS.md). The
current binding balance contract remains latest-only and `assets[]`-based
until SPEC-012 is implemented and the contract, OpenAPI, examples, smoke
checks, and [HISTORY.md](../../HISTORY.md) are updated in the same change.

Breaking the private Beta balance contract is acceptable for v0.3 because the
resulting surface should be clearer, less catalog-bound, and harder to
misinterpret.

## Goal

`POST /v1/balances` and `POST /v1/balances/bulk` should become v0.3 balance
endpoints that can request:

- latest or historical balances through explicit `as_of`;
- catalog-backed assets through `tokens.asset_slugs`;
- explicit ERC-20 token contracts through `tokens.contract_addresses`;
- quote values only when pricing is supported and time-aligned.

The v0.3 request replaces `assets[]` with `tokens`. The legacy `assets[]` shape
is not accepted as an alias.

## Ownership

Mother API owns:

- public HTTP DTOs, validation, OpenAPI, examples, and error mapping;
- canonical `network_slug` validation and catalog resolution;
- mapping `asset_slug` plus `network_slug` to concrete balance targets;
- resolving explicit ERC-20 contract addresses back to known catalog assets
  when possible;
- response shaping that separates raw balance availability from quote
  availability.

Bigwig, or another accepted internal upstream contract, owns balance evidence,
provider access, `eth_call` behavior, block selection, timestamp-to-block
resolution, and historical balance availability.

Price Indexer owns quote availability, derivation, freshness, FX conversion,
and historical price data. Mother API consumes it read-only.

Mother API must not perform direct EVM JSON-RPC calls, price indexing,
timestamp-to-block resolution, event indexing, holder indexing, or in-process
response caching for this feature.

## Request Contract Direction

Both balance endpoints use the existing single-account and bulk-account
shapes, except that `assets[]` is replaced by `tokens`:

```json
{
  "as_of": {
    "kind": "latest"
  },
  "account": {
    "network_slug": "eth-mainnet",
    "address": "0x1234567890abcdef1234567890abcdef1234beef",
    "client_ref": "treasury-main"
  },
  "quote_currency": "USD",
  "tokens": {
    "asset_slugs": ["usdc", "wrapped-ether"],
    "contract_addresses": [
      "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
    ]
  }
}
```

`network_slug` remains the only public network identity. The v0.3 contract must
not accept `chain`, `chain_id`, or `chain_slug` as aliases.

`tokens` follows the ERC-20 transfer-search style where practical:

- `tokens.asset_slugs[]` contains exact canonical Mother API asset slugs.
- `tokens.contract_addresses[]` contains concrete `0x` 20-byte EVM ERC-20
  contract addresses.
- At least one selector must be non-empty.
- Duplicate selectors are normalized and deduplicated before upstream balance
  work where practical.
- If an `asset_slug` and `contract_address` resolve to the same token, Mother
  API should avoid duplicate upstream balance work while preserving clear
  response attribution.

Unknown catalog `asset_slug` values remain request-level
`400 unsupported_asset`. Invalid contract-address syntax remains request-level
validation failure. Non-ERC-20-compatible contracts and upstream balance-call
failures remain item-level failures when upstream evidence supports that.

## `as_of`

V0.3 supports these public forms:

```json
{ "kind": "latest" }
```

```json
{ "kind": "timestamp", "timestamp": "2026-07-03T00:00:00Z" }
```

```json
{ "kind": "block_number", "block_number": "19000000" }
```

Historical implementation is gated on an accepted Bigwig or equivalent
internal historical-balance contract. Until that upstream contract exists,
SPEC-012 remains draft and historical requests must not be exposed as a public
promise.

Historical requests must never silently fall back to latest balances. If the
requested historical evidence is unavailable, the affected account or token
result must report explicit unavailability.

For `block_number`, block numbers are network-local. The first v0.3
implementation may reject mixed-network bulk requests for `block_number`
unless the accepted upstream contract supports a network-to-block mapping.

## Quote Semantics

Balance availability and quote availability are separate.

Quote enrichment may be attempted when the requested token is a catalog
`asset_slug`, or an explicit contract address resolves to a known catalog
asset, and Price Indexer has a usable price for the requested quote currency
and `as_of`.

Quote enrichment is `unsupported` when an explicit contract address cannot be
resolved to a known catalog asset. The raw balance may still be returned.

Quote enrichment is `unavailable` when the token is known but a usable price
does not exist, the price provider is unavailable, the historical price is
missing, or the price is stale under the accepted pricing policy.

Quote statuses should align with the current balance response style:

```text
available
unavailable
unsupported
```

Historical balance requests must not silently use latest prices. If no
time-aligned historical price exists, return the raw balance and mark the quote
as `unavailable`.

## Response Direction

The v0.3 response should preserve the current balance response structure where
possible while adding the minimum fields needed to distinguish:

- requested token selector and resolved token identity;
- raw balance availability;
- decimal metadata availability;
- quote availability;
- requested `as_of` and resolved evidence point.

Response evidence may expose public block number, block hash, and observed or
resolved timestamps when upstream evidence provides them. It must not expose
Bigwig routes, providers, URLs, chain IDs, authentication details, price
internals, or operation-capability details.

## Validation and Errors

Request-level errors are for invalid input or impossible request shapes:

- malformed or non-JSON body;
- unknown fields;
- reserved network alias fields;
- unsupported `as_of.kind`;
- invalid timestamp or block number;
- unsupported `network_slug`;
- invalid account address;
- empty `tokens`;
- unknown `asset_slug`;
- invalid contract-address syntax;
- unsupported quote currency;
- duplicate account;
- request too large;
- mixed-network `block_number` request when unsupported.

Item-level failures are for supported requests whose balance or quote evidence
cannot be resolved:

- upstream balance evidence unavailable;
- ERC-20 balance call failed;
- contract is not ERC-20-compatible;
- decimals unavailable;
- historical balance unavailable;
- timestamp-to-block resolution unavailable upstream;
- quote unavailable or unsupported.

Public errors and item-level errors must remain sanitized Mother API errors.
They must not leak upstream provider topology or pricing internals.

## Implementation Notes

- Update balance DTOs and examples to use `tokens`, not `assets[]`.
- Reuse the existing token-filter validation style from ERC-20 transfer
  search where it fits the balance contract.
- Reuse existing catalog helpers for resolving explicit ERC-20 contract
  addresses to known assets when possible.
- Extend balance orchestration only through accepted upstream balance evidence
  contracts.
- Update `CONTRACTS.md`, README/private-Beta examples, generated OpenAPI,
  smoke checks, and `HISTORY.md` in the implementation change.

## Implementation PR Breakdown

### PR 1 - V0.3 DTOs, Validation, and Draft OpenAPI Review

- Replace balance request DTOs and reusable examples with the `tokens` shape.
- Reject legacy `assets[]`, unknown fields, reserved network aliases, empty
  token selectors, invalid contract addresses, and unsupported `as_of` forms
  not yet backed by upstream evidence.
- Update the generated OpenAPI schema and examples behind the draft v0.3 contract
  so reviewers can inspect the intended public surface.
- Add OpenAPI snapshot/contract tests proving:
  - `tokens.asset_slugs[]` is documented;
  - `tokens.contract_addresses[]` is documented;
  - `assets[]` is no longer present in the v0.3 request schema;
  - unsupported historical `as_of` variants are not accidentally documented
    as enabled runtime behavior.
- Do not enable the breaking runtime contract yet unless the repository has a
  feature flag or explicit beta-surface switch for reviewing draft contracts.

### PR 2 - Latest Balance Token Selector Orchestration

- Resolve `tokens.asset_slugs` through the existing catalog-backed balance
  target resolver.
- Resolve `tokens.contract_addresses` as explicit ERC-20 balance targets and
  enrich them with catalog metadata when available.
- Deduplicate equivalent asset-slug and contract-address targets before
  upstream work where practical.
- Keep unresolved explicit contracts eligible for raw balance results with
  `unsupported` quote status.

### PR 3 - Quote and Response Shaping

- Extend balance responses only as needed to expose requested token identity,
  resolved token identity, metadata availability, raw balance status, and
  quote status.
- Keep quote statuses aligned to `available`, `unavailable`, and
  `unsupported`.
- Ensure missing, stale, unsupported, or provider-unavailable quote data does
  not hide otherwise valid raw balance evidence.

### PR 4 - Enable Binding V0.3 Contract, Docs, OpenAPI, and Smoke Coverage

- Enable the v0.3 breaking balance contract for the private Beta surface.
- Update `CONTRACTS.md` so `tokens` is the binding request shape and `assets[]`
  is explicitly removed.
- Regenerate and commit OpenAPI from the enabled runtime contract.
- Update README/private-Beta examples to match the generated OpenAPI.
- Update smoke checks to send `tokens`, not `assets[]`.
- Update `HISTORY.md` with the private Beta breaking change.
- Add route, contract, and OpenAPI tests proving:
  - `assets[]` is rejected;
  - `tokens` is accepted;
  - generated OpenAPI matches the enabled runtime DTOs;
  - smoke examples are synchronized with the public contract;
  - public errors remain sanitized.

### PR 5 - Historical Balances

- Implement historical `as_of` only after Bigwig or another accepted upstream
  contract exposes historical balance evidence.
- Add timestamp and block-number route coverage proving historical requests
  never fall back to latest balances.
- Add historical quote tests proving latest prices are not silently used for
  historical balance requests.

## Test Plan

- DTO tests reject `assets[]`, accept `tokens`, reject unknown fields, reject
  invalid contract addresses, and validate duplicate filters.
- Route tests cover latest catalog assets, latest explicit ERC-20 contracts,
  unresolved contract quote status, and mixed selector deduplication.
- Contract and OpenAPI tests prove the generated balance schema exposes
  `tokens` and no longer exposes `assets[]`.
- Historical tests are added only after the accepted upstream historical
  balance contract exists:
  - timestamp requests use upstream historical evidence;
  - block-number requests honor the requested block exactly;
  - unavailable historical evidence is explicit;
  - latest-balance fallback is impossible.

## Assumptions

- Breaking the private Beta balance contract is acceptable.
- `assets[]` is removed immediately and is not a compatibility alias.
- SPEC-012 remains draft until the replacement contract and upstream
  historical support are ready.
- Mother API does not expand into direct EVM RPC, price indexing, or
  timestamp-to-block ownership.
