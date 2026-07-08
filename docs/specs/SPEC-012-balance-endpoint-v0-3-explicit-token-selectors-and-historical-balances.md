---
status: draft
owner: iron-burrow
last_reviewed: 2026-07-07
agent_edit_policy: update_when_relevant
---

# SPEC-012 - Balance Endpoint v0.3 Explicit Token Selectors and Historical Balances

Draft replacement spec for the private Beta balance endpoints.

This document does not override [CONTRACTS.md](../../CONTRACTS.md). The
current binding balance contract remains latest-only, and only slices already
reflected in `CONTRACTS.md` are public truth until later SPEC-012 slices are
implemented and the contract, OpenAPI, examples, smoke checks, and
[HISTORY.md](../../HISTORY.md) are updated in the same change.

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

For SPEC-012 historical balances, Bigwig SPEC-009 is implemented and accepted.
Mother API should use Bigwig's internal v2 primitive route:

`POST /internal/v1/primitives/evm/balances`

Requests to this route must include hub-only internal authentication and
service identity:

- `Authorization: Bearer <INFRA_GATEWAY_TOKEN>`
- `X-Client-Service: mother-api`

Current SPEC-009 historical coverage for this route is:

- `eth-mainnet`
- `base-mainnet`

Bigwig accepts one network per request. Mother API should group balance work
by `network_slug` before calling Bigwig.

Bigwig accepts only concrete balance targets:

- native token: `{ "kind": "native" }`
- ERC-20 token: `{ "kind": "erc20", "contract_address": "0x..." }`

Bigwig does not accept public selector forms or metadata such as `asset_slug`,
symbol, decimals, quote currency, prices, or catalog annotations. Mother API
must continue to own selector resolution, catalog and decimal enrichment,
quote enrichment, and final public response shaping.

Price Indexer owns quote availability, derivation, freshness, FX conversion,
and historical price data. Mother API consumes it read-only.

Mother API must not perform direct EVM JSON-RPC calls, price indexing,
timestamp-to-block resolution, event indexing, holder indexing, or in-process
response caching for this feature.

## Request Contract Direction

Both balance endpoints use the existing single-account and bulk-account
shapes, with account identity grouped under `account` or `accounts[]` and
token intent grouped under `tokens`:

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

## Account Identity Alignment

Mother API account-scoped public endpoints should use a shared account
identity shape:

```json
{
  "account": {
    "network_slug": "eth-mainnet",
    "address": "0xabc0000000000000000000000000000000000000",
    "client_ref": "optional-ref"
  }
}
```

`network_slug` remains the canonical public network identifier, but for
account-scoped public endpoints it should live under `account` rather than as
a top-level field. `account.address` is the watched or balance-owning EVM
account address, not an ERC-20 token contract address. `account.client_ref` is
optional caller-defined metadata that Mother API may echo in public responses
when present.

This shape already matches the intended balance request and response direction.
A future breaking change should homologate `POST /v1/erc20-transfers/search`
to the same public account shape in both request and response:

- replace top-level public `network_slug` and `address` with `account`;
- accept optional `account.client_ref` and echo it unchanged when present;
- reject top-level public `network_slug` and `address` rather than keeping
  compatibility aliases;
- keep transfer `tokens.contract_addresses[]` distinct from
  `account.address`;
- keep Bigwig's accepted internal ERC-20 transfer extraction contract
  unchanged.

Until that ERC-20 transfer change is implemented, this section is planning
guidance only. The binding ERC-20 transfer public contract remains
[CONTRACTS.md](../../CONTRACTS.md), and Mother API may unwrap the future public
`account` object into Bigwig's existing internal top-level `network_slug` and
`address` fields.

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

Historical implementation is no longer blocked on upstream contract readiness:
Bigwig SPEC-009 now provides accepted historical-balance evidence via
`POST /internal/v1/primitives/evm/balances`.

Historical requests must never silently fall back to latest balances. If the
requested historical evidence is unavailable, the affected account or token
result must report explicit unavailability.

For `block_number`, block numbers are network-local. The first v0.3
implementation may reject mixed-network bulk requests for `block_number`
unless the accepted upstream contract supports a network-to-block mapping.

Bigwig `as_of` handling relevant to SPEC-012 is:

- `latest`: Bigwig pins an observed head block before executing balances.
- `block_number`: Bigwig executes at the exact requested block.
- `timestamp`: Bigwig resolves to the highest block whose timestamp is less
  than or equal to the requested timestamp, then executes at that block.

Historical requests must never fall back to latest.

Canonical Bigwig request shape for historical and latest evidence work:

```json
{
  "network_slug": "eth-mainnet",
  "as_of": {
    "kind": "block_number",
    "block_number": "19000000"
  },
  "accounts": [
    "0x1234567890abcdef1234567890abcdef1234beef"
  ],
  "tokens": [
    {
      "kind": "native"
    },
    {
      "kind": "erc20",
      "contract_address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
    }
  ]
}
```

Bigwig `as_of` variants consumed by Mother API:

```json
{ "kind": "latest" }
```

```json
{ "kind": "block_number", "block_number": "19000000" }
```

```json
{ "kind": "timestamp", "timestamp": "2026-07-03T00:00:00Z" }
```

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

For this Bigwig primitive, Mother API should model and preserve Bigwig
`resolved_evidence` internally (including resolved `block_number`,
`block_hash`, and `block_timestamp`) so public responses can clearly describe
the actual evidence block used for balance resolution.

Canonical Bigwig response shape to model internally:

```json
{
  "primitive": "evm_balances",
  "status": "complete",
  "network": {
    "network_slug": "eth-mainnet",
    "chain_id": 1
  },
  "requested_as_of": {
    "kind": "block_number",
    "block_number": "19000000"
  },
  "resolved_evidence": {
    "kind": "exact_block",
    "block_number": "19000000",
    "block_hash": "0x...",
    "block_timestamp": "2024-01-15T12:34:56Z"
  },
  "items": [
    {
      "account_address": "0x1234567890abcdef1234567890abcdef1234beef",
      "requested_token": {
        "kind": "erc20",
        "contract_address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
      },
      "raw_balance": {
        "status": "resolved",
        "value": "123456789"
      }
    }
  ]
}
```

For this response family, top-level `status` is `complete`, `partial`, or
`failed`. Item-level `raw_balance.status` is `resolved`, `failed`, or
`unavailable`.

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

When Bigwig reasons are available, Mother API should preserve/map at least:

- `erc20_contract_code_absent_at_evidence_block`;
- `erc20_balanceof_not_supported`;
- `historical_evidence_unavailable`;
- `native_balance_call_failed`;
- `erc20_balance_call_failed`.

Request-wide Bigwig errors should be mapped into sanitized Mother API public
errors while preserving actionable semantics for operators, including:

- `invalid_as_of`;
- `unsupported_network`;
- `network_not_enabled_for_operation`;
- `no_route_satisfies_operation`;
- `block_out_of_range`;
- `timestamp_anchor_not_configured`;
- `timestamp_out_of_range`;
- `gateway_rate_limited`;
- `provider_unavailable`;
- `provider_timeout`.

Public errors and item-level errors must remain sanitized Mother API errors.
They must not leak upstream provider topology or pricing internals.

## Implementation Notes

- Update balance DTOs and examples to use `tokens`, not `assets[]`.
- Update Bigwig client DTOs for
  `POST /internal/v1/primitives/evm/balances`.
- Reuse the existing token-filter validation style from ERC-20 transfer
  search where it fits the balance contract.
- Align future account-scoped public request and response shapes on
  `account.network_slug`, `account.address`, and optional
  `account.client_ref`.
- Resolve SPEC-012 public token selectors (`tokens.asset_slugs` and
  `tokens.contract_addresses`) into concrete Bigwig token targets before
  calling Bigwig.
- Reuse existing catalog helpers for resolving explicit ERC-20 contract
  addresses to known assets when possible.
- Keep decimals, catalog metadata, and quote enrichment Mother-owned; do not
  expect Bigwig to return them.
- Extend balance orchestration only through accepted upstream balance evidence
  contracts.
- Update `CONTRACTS.md`, README/private-Beta examples, generated OpenAPI,
  smoke checks, and `HISTORY.md` in the implementation change.

## Implementation PR Breakdown

### PR 0 - Public Account Shape Documentation Alignment

- Update SPEC-012 with the cross-endpoint account identity direction for
  account-scoped public endpoints.
- Keep this PR documentation-only: no runtime DTOs, route behavior, generated
  OpenAPI, smoke checks, or binding contract examples change.
- Do not update `CONTRACTS.md`, README quickstarts, runbooks, smoke docs,
  OpenAPI examples, or `HISTORY.md` until the matching runtime change lands.
- If SPEC-007 is touched, add only a forward-looking note that a future
  private-Beta breaking change may move ERC-20 transfer search to the shared
  `account` wrapper; do not rewrite SPEC-007 as though that shape is already
  implemented.

### PR 1 - V0.3 DTOs, Validation, and Draft OpenAPI Review

- Replace balance request DTOs and reusable examples with the `tokens` shape.
- Reject legacy `assets[]`, unknown fields, reserved network aliases, empty
  token selectors, invalid contract addresses, and unsupported `as_of` forms.
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

### PR 2 - ERC-20 Transfer Account Shape Homologation

- Change the public ERC-20 transfer request and response DTOs to use
  `account.network_slug`, `account.address`, and optional
  `account.client_ref`.
- Remove top-level public `network_slug` and `address`; do not keep them as
  request aliases.
- Preserve the accepted internal Bigwig transfer extraction request shape by
  unwrapping public `account` into Bigwig's existing top-level
  `network_slug` and `address`.
- Update `CONTRACTS.md`, README/private-Beta quickstarts, runbooks, smoke
  checks, generated OpenAPI, examples, route tests, and `HISTORY.md` in the
  same runtime change.
- Add tests proving public responses echo `account.client_ref` when present
  and do not expose top-level public `network_slug` or `address`.

### PR 3 - Latest Balance Token Selector Orchestration

- Resolve `tokens.asset_slugs` through the existing catalog-backed balance
  target resolver.
- Resolve `tokens.contract_addresses` as explicit ERC-20 balance targets and
  enrich them with catalog metadata when available.
- Deduplicate equivalent asset-slug and contract-address targets before
  upstream work where practical.
- Keep unresolved explicit contracts eligible for raw balance results with
  `unsupported` quote status.

### PR 4 - Quote and Response Shaping

- Extend balance responses only as needed to expose requested token identity,
  resolved token identity, metadata availability, raw balance status, and
  quote status.
- Keep quote statuses aligned to `available`, `unavailable`, and
  `unsupported`.
- Ensure missing, stale, unsupported, or provider-unavailable quote data does
  not hide otherwise valid raw balance evidence.

### PR 5 - Enable Binding V0.3 Contract, Docs, OpenAPI, and Smoke Coverage

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

### PR 6 - Historical Balances

- Implement historical `as_of` using the accepted Bigwig primitive route
  `POST /internal/v1/primitives/evm/balances`.
- Add timestamp and block-number route coverage proving historical requests
  never fall back to latest balances.
- Add latest-route coverage proving `as_of.kind=latest` pins a concrete
  evidence block before balances execute.
- Add coverage for partial item failures, including
  `erc20_contract_code_absent_at_evidence_block`.
- Add coverage for unavailable historical evidence and request-wide historical
  range failures.
- Add historical quote tests proving latest prices are not silently used for
  historical balance requests.

## Test Plan

- DTO tests reject `assets[]`, accept `tokens`, reject unknown fields, reject
  invalid contract addresses, and validate duplicate filters.
- Route tests cover latest catalog assets, latest explicit ERC-20 contracts,
  unresolved contract quote status, and mixed selector deduplication.
- Contract and OpenAPI tests prove the generated balance schema exposes
  `tokens` and no longer exposes `assets[]`.
- ERC-20 transfer account-shape tests are added only in the runtime
  homologation PR:
  - requests require `account.network_slug` and `account.address`;
  - responses echo `account` and optional `account.client_ref`;
  - top-level public `network_slug` and `address` are rejected or absent;
  - Bigwig still receives its accepted internal top-level network and address
    fields.
- Historical tests for the accepted Bigwig upstream historical contract cover:
  - timestamp requests use upstream historical evidence;
  - block-number requests honor the requested block exactly;
  - unavailable historical evidence is explicit;
  - partial Bigwig item failures are preserved/mapped;
  - latest-balance fallback is impossible.

## Assumptions

- Breaking the private Beta balance contract is acceptable.
- `assets[]` is removed immediately and is not a compatibility alias.
- The future ERC-20 transfer account wrapper is a private-Beta breaking
  change and will not keep top-level public `network_slug` or `address`
  compatibility aliases.
- SPEC-012 remains draft until the replacement contract and upstream
  historical support are fully shipped to the private Beta public contract.
- Initial upstream historical network coverage for this SPEC-012 slice is
  `eth-mainnet` and `base-mainnet`.
- `account.client_ref` is Mother API public metadata only; it is not forwarded
  to Bigwig or used for extraction, catalog resolution, token filtering, or
  limits.
- Mother API does not expand into direct EVM RPC, price indexing, or
  timestamp-to-block ownership.
