---
status: accepted
owner: iron-burrow
last_reviewed: 2026-06-18
agent_edit_policy: update_when_relevant
external_contract:
  - iron-burrow-price-indexer/CONTRACTS.md@2026-06-02
  - iron-burrow-infra-gateway/CONTRACTS.md@3.5.0
---

# SPEC-006 - Network-Scoped Balances v1

Accepted implementation target for Mother API endpoints that resolve latest
balances for one or more network-scoped EVM addresses across canonical Iron
Burrow assets.

The public routes are implemented and their binding promises are captured in
[CONTRACTS.md](../../CONTRACTS.md). This spec remains the accepted design and
implementation record; `CONTRACTS.md` is authoritative for public callers.

The Bigwig EVM latest balance evidence primitive is available in production as
of Bigwig 3.5.0. Mother API SPEC-006 is aligned with that binding contract.

## Purpose

This spec defines a compact balance snapshot surface for Coto / Control Tower.
A caller provides explicit addresses, networks, assets, and a quote currency,
then receives latest raw balances, decimal amounts, and quote values.

One request may contain accounts from multiple networks. A bare address never
implies every network, and Mother API must not create address-network pairs the
caller did not request.

V1 is limited to latest EVM balances.

## Core definitions

### Network-scoped address

A balance subject is:

```txt
(network_slug, address)
```

The same EVM address may represent different deployments or ownership contexts
on different networks. Callers must provide each desired pair explicitly.

### Canonical network slug

Mother API and Bigwig share one canonical Iron Burrow `network_slug`
namespace for EVM balance resolution. Canonical EVM slugs include, but are not
limited to:

```txt
eth-mainnet
base-mainnet
mantle-mainnet
arbitrum-mainnet
```

A canonical EVM network is eligible for SPEC-006 balance resolution when:

1. the network exists in Mother's canonical network catalog;
2. the network has active asset mappings for the requested assets;
3. the network is expected to be callable through Bigwig's 3.5.0
   latest-balance primitive.

Mother must not treat the example slug list as a hard-coded allowlist or
duplicate Bigwig's operation-capable route map. Mother sends an accepted
`network_slug` unchanged to Bigwig. Bigwig owns internal operation-aware route
resolution, and Mother must not send or receive `route_id`.

If Bigwig returns `unsupported_network`, `network_not_enabled_for_operation`,
or `no_route_satisfies_operation` for a Mother-accepted network, Mother maps
that as a runtime balance resolution failure according to the Bigwig error
mapping section. Mother must not expose route or provider details.

`eth-mainnet` is a canonical EVM network slug and must not be rejected merely
because earlier examples focused on Base and Arbitrum. If Mother has active
catalog mappings for the requested assets and Bigwig has an operation-capable
latest-balance route for `eth-mainnet`, Mother must accept and orchestrate the
request.

Mother's current seeded catalog may contain legacy values including `base`,
`mantle`, and `arbitrum-one`. Before enabling the public endpoints, the
implementation must migrate those catalog values and their asset mappings to
canonical `*-mainnet` slugs.

Legacy slugs are not balance aliases in v1. Requests using legacy slugs must
return `unsupported_network`.

`bitcoin-mainnet` may remain in the catalog, but non-EVM balance resolution is
out of scope for v1. Non-EVM and unknown slugs return `unsupported_network`
before orchestration.

### Canonical asset slug

`asset_slug` is the network-agnostic identifier from
`mother_api.global_asset.slug`, for example:

```txt
usdc
bitso-mxn
ethereum
wrapped-ether
wrapped-bitcoin
```

Symbols and aliases are not request slugs. Mother owns the mapping:

```txt
(asset_slug, network_slug) -> active network-specific asset metadata
```

The metadata supplies native status, ERC-20 contract address, decimals,
symbol, display name, pricing identity, and related catalog fields. Callers
must not provide this metadata.

The catalog invariant is that one active concrete target identifies at most one
canonical asset on a network:

```txt
one active native target per network
one active ERC-20 contract address per network
```

The Bigwig adapter must still deduplicate targets defensively.

## Ownership and provider boundary

Coto / Control Tower owns watched-account metadata:

```txt
network_slug + address + label/tags/client_ref
```

Mother API owns public validation, catalog lookup, latest-balance
orchestration, price enrichment, and response shaping.

The service split is:

```txt
Mother API resolves:
  asset_slug + network_slug -> native/ERC-20 concrete target

Bigwig resolves:
  network_slug + account address + concrete target
    -> internal operation-capable route
    -> raw_amount evidence

Price Indexer resolves:
  asset quote price, FX, and quote value inputs
```

Mother API must not send `asset_slug`, symbols, decimals, quote currencies,
prices, `client_ref`, Coto concepts, or Mother catalog metadata to Bigwig. A
Bigwig request contains only `network_slug`, `accounts[].address`, and concrete
`targets`.

Mother API must not perform direct EVM JSON-RPC calls, price derivation,
balance indexing, holder indexing, DeFi protocol math, or protocol-specific
reserve lookup.

## Public endpoints

```http
POST /v1/balances
POST /v1/balances/bulk
```

The single-account endpoint is syntactic sugar over the bulk orchestration
model with one account. Both endpoints use:

```txt
BalanceSnapshotService::resolve_latest(request)
```

with these internal adapters:

```txt
CatalogBalanceTargetResolver
BigwigClient
PriceQuoteClient
BalancesResponsePresenter
```

These routes are implemented and are part of the public contract in
[CONTRACTS.md](../../CONTRACTS.md).

## Request model

Supported in v1:

- EVM network-scoped addresses only;
- latest balances only;
- caller-supplied accounts and canonical asset slugs;
- quote values for supported quote currencies.

Single-account request:

```json
{
  "as_of": {
    "kind": "latest"
  },
  "account": {
    "network_slug": "arbitrum-mainnet",
    "address": "0x1234567890abcdef1234567890abcdef1234beef",
    "client_ref": "main-safe-arbitrum"
  },
  "quote_currency": "MXN",
  "assets": [
    {
      "asset_slug": "usdc"
    }
  ]
}
```

Bulk request:

```json
{
  "as_of": {
    "kind": "latest"
  },
  "accounts": [
    {
      "network_slug": "base-mainnet",
      "address": "0x1234567890abcdef1234567890abcdef1234beef",
      "client_ref": "treasury-base"
    },
    {
      "network_slug": "arbitrum-mainnet",
      "address": "0xabcdefabcdefabcdefabcdefabcdefabcdefabcd",
      "client_ref": "ops-wallet-arbitrum"
    }
  ],
  "quote_currency": "MXN",
  "assets": [
    {
      "asset_slug": "usdc"
    },
    {
      "asset_slug": "bitso-mxn"
    }
  ]
}
```

Resolution is the explicit cartesian product:

```txt
accounts[] x assets[]
```

Mother must not infer assets or expand an address to unrequested networks.

Only this `as_of` value is supported:

```json
{
  "kind": "latest"
}
```

Historical requests return `unsupported_as_of`.

## Validation and limits

Mother validates public concepts before orchestration:

```txt
invalid_account
unsupported_network
unsupported_asset
unsupported_quote_currency
unsupported_as_of
empty_accounts
empty_assets
duplicate_account
duplicate_asset
request_too_large
```

For EVM networks, an address must be exactly `0x` plus 40 ASCII hex
characters. EIP-55 checksum validation is not enforced in v1. Responses echo
caller-provided address casing.

Duplicate accounts compare:

```txt
(network_slug, lower(address))
```

Duplicate assets compare `asset_slug`.

Mother enforces these public limits before orchestration:

```txt
max_accounts: 50
max_assets: 20
max_resolution_items: 1000
```

Where:

```txt
resolution_items = accounts.length x assets.length
```

After grouping by `network_slug`, every Bigwig request must also satisfy
Bigwig 3.5.0 limits:

```txt
max_accounts: 50
max_targets: 20
max_account_target_items: 1000
```

Mother returns `request_too_large` if the public request or any grouped Bigwig
call would exceed its applicable limits. Mother must reject an oversized
request; it must not chunk one public request into multiple Bigwig calls for
the same network.

## Bigwig integration

### Bigwig 3.5.0 adapter

Bigwig accepts exactly one network per request. Mother must group accounts by
`network_slug` exactly once, preserve caller account order within each group,
and make at most one Bigwig call for each non-empty network target group.

For each group, Mother must:

1. collect the requested accounts for that network;
2. resolve each requested `asset_slug` to an active network-specific target;
3. skip unsupported asset-network pairs before calling Bigwig;
4. deduplicate concrete targets while preserving first-occurrence order;
5. call `POST /internal/v1/primitives/evm/latest-balances`;
6. map evidence items back to Mother positions using catalog metadata.

If every requested asset is unsupported on a network, Mother must not call
Bigwig for that group. The affected account results contain their skipped
items and use `"evidence": null`.

Mother-to-Bigwig request:

```json
{
  "network_slug": "arbitrum-mainnet",
  "accounts": [
    {
      "address": "0x1234567890abcdef1234567890abcdef1234beef"
    }
  ],
  "targets": [
    {
      "kind": "erc20",
      "contract_address": "0xaf88d065e77c8cc2239327c5edb3a432268e5831"
    },
    {
      "kind": "native"
    }
  ]
}
```

Target mapping:

```txt
native asset on an EVM network:
  { "kind": "native" }

ERC-20 asset on an EVM network:
  {
    "kind": "erc20",
    "contract_address": "<catalog token contract address>"
  }
```

Mother retains this correlation while assembling the response:

```txt
(network_slug, lower(account_address), concrete_target) -> asset_slug
```

Bigwig duplicate target identity is `kind:native` or
`kind:erc20 + lower(contract_address)`. Response correlation uses only the
normalized account address and concrete target. It must not depend on
`asset_slug`, response position alone, route identity, or provider metadata.

### Bigwig response validation

For every successful Bigwig response, Mother must validate:

```txt
primitive = evm_latest_balances
network.network_slug = requested network_slug
network.chain_id = expected catalog chain ID
items.length = grouped accounts.length x deduplicated targets.length
each requested account-target correlation appears exactly once
item order follows accounts as the outer loop and targets as the inner loop
status matches the resolved and failed item counts
```

Expected chain IDs must come from Mother's canonical network catalog, not from
a hard-coded network list.

Examples:

```txt
eth-mainnet: 1
base-mainnet: 8453
arbitrum-mainnet: 42161
mantle-mainnet: 5000
```

Mother must validate that Bigwig's returned `network.chain_id` matches the
catalog chain ID for the requested `network_slug`. `chain_id` is an internal
consistency check and is not added to Mother's public evidence shape.
A malformed success body, unexpected correlation, duplicate or missing item,
or inconsistent status is a Mother `internal_error`; the affected account
results use `"evidence": null`.

Bigwig pins all items in one call to one observed block. Mother may propagate
that block number, block hash, and `observed_at`, but this is latest snapshot
evidence and makes no finality guarantee.

### Bigwig error mapping

Request-wide Bigwig failures map as follows:

| Bigwig failure | Mother result |
| --- | --- |
| `401 unauthorized` | `balance_provider_unavailable`; never expose authentication details |
| `404 unsupported_network` after Mother admission | `balance_resolution_failed` |
| `422 network_not_enabled_for_operation` | `balance_resolution_failed` |
| `422 no_route_satisfies_operation` | `balance_resolution_failed` |
| `429 gateway_rate_limited` | `balance_provider_unavailable`; retain `Retry-After` internally |
| `502 rpc_error` | `balance_resolution_failed` |
| `503 provider_unavailable` | `balance_provider_unavailable` |
| `504 provider_timeout` | `balance_provider_unavailable` |
| `500 internal_error` | `balance_provider_unavailable` |
| transport failure or client timeout | `balance_provider_unavailable` |

Any Bigwig malformed-body or request-validation error after Mother accepted
and shaped the request is a Mother `internal_error`.

A request-wide Bigwig failure affects every supported account-target item in
that network group, and each affected pair receives the mapped Mother error
code in its account result. Other network groups may still succeed, allowing a
bulk response to be `partial`. Request-wide failures do not establish balance
evidence, so affected account results use `"evidence": null`.

A valid Bigwig `partial` or `failed` evidence envelope did establish a pinned
snapshot. Mother preserves its block and observation evidence while mapping
the failed items.

Mother public responses must not expose Bigwig route IDs, provider names,
provider URLs, node roles, capability versions, route evidence, API keys,
authentication details, or sanitized upstream internals.

### Bigwig item failures

Bigwig item-level codes are:

```txt
native_balance_call_failed
erc20_balance_call_failed
erc20_bad_response
```

Mother maps all three to a public item error:

```json
{
  "network_slug": "arbitrum-mainnet",
  "asset_slug": "usdc",
  "code": "balance_resolution_failed",
  "message": "Balance could not be resolved for this asset on this network."
}
```

Bigwig item codes remain in structured logs and metrics; Mother does not expose
a public `provider_code`.

## Response model

### Single account

```json
{
  "ok": true,
  "type": "balances",
  "status": "complete",
  "as_of": {
    "kind": "latest",
    "observed_at": "2026-06-16T15:04:30Z"
  },
  "quote_currency": "MXN",
  "account": {
    "network_slug": "arbitrum-mainnet",
    "address": "0x1234567890abcdef1234567890abcdef1234beef",
    "client_ref": "main-safe-arbitrum"
  },
  "evidence": {
    "source": "bigwig",
    "network_slug": "arbitrum-mainnet",
    "block": {
      "number": "123456789",
      "hash": "0x0000000000000000000000000000000000000000000000000000000000000000"
    },
    "observed_at": "2026-06-16T15:04:30Z"
  },
  "positions": [
    {
      "network_slug": "arbitrum-mainnet",
      "asset_slug": "usdc",
      "symbol": "USDC",
      "balance": {
        "raw_amount": "8000123456",
        "amount": "8000.123456",
        "decimals": 6
      },
      "quote": {
        "status": "available",
        "currency": "MXN",
        "unit_price": "18.45",
        "value": "147602.2777632",
        "price_as_of": "2026-06-16T15:03:59Z"
      }
    }
  ],
  "skipped": [],
  "errors": []
}
```

For a single-account response, `as_of.observed_at` comes from Bigwig and equals
`evidence.observed_at`.

### Bulk accounts

```json
{
  "ok": true,
  "type": "balances_bulk",
  "status": "complete",
  "as_of": {
    "kind": "latest"
  },
  "quote_currency": "MXN",
  "summary": {
    "requested_accounts": 2,
    "requested_assets": 2,
    "requested_resolution_items": 4,
    "positions_returned": 3,
    "skipped_items": 1,
    "failed_items": 0
  },
  "accounts": [
    {
      "status": "complete",
      "account": {
        "network_slug": "base-mainnet",
        "address": "0x1234567890abcdef1234567890abcdef1234beef",
        "client_ref": "treasury-base"
      },
      "evidence": {
        "source": "bigwig",
        "network_slug": "base-mainnet",
        "block": {
          "number": "31234567",
          "hash": "0x1111111111111111111111111111111111111111111111111111111111111111"
        },
        "observed_at": "2026-06-16T15:04:29Z"
      },
      "positions": [
        {
          "network_slug": "base-mainnet",
          "asset_slug": "usdc",
          "symbol": "USDC",
          "balance": {
            "raw_amount": "12000123456",
            "amount": "12000.123456",
            "decimals": 6
          },
          "quote": {
            "status": "available",
            "currency": "MXN",
            "unit_price": "18.45",
            "value": "221402.2777632",
            "price_as_of": "2026-06-16T15:03:59Z"
          }
        }
      ],
      "skipped": [
        {
          "network_slug": "base-mainnet",
          "asset_slug": "bitso-mxn",
          "reason": "asset_not_supported_on_network"
        }
      ],
      "errors": []
    },
    {
      "status": "complete",
      "account": {
        "network_slug": "arbitrum-mainnet",
        "address": "0xabcdefabcdefabcdefabcdefabcdefabcdefabcd",
        "client_ref": "ops-wallet-arbitrum"
      },
      "evidence": {
        "source": "bigwig",
        "network_slug": "arbitrum-mainnet",
        "block": {
          "number": "123456789",
          "hash": "0x0000000000000000000000000000000000000000000000000000000000000000"
        },
        "observed_at": "2026-06-16T15:04:30Z"
      },
      "positions": [
        {
          "network_slug": "arbitrum-mainnet",
          "asset_slug": "usdc",
          "symbol": "USDC",
          "balance": {
            "raw_amount": "450000000",
            "amount": "450.000000",
            "decimals": 6
          },
          "quote": {
            "status": "available",
            "currency": "MXN",
            "unit_price": "18.45",
            "value": "8302.500000",
            "price_as_of": "2026-06-16T15:03:59Z"
          }
        },
        {
          "network_slug": "arbitrum-mainnet",
          "asset_slug": "bitso-mxn",
          "symbol": "MXNB",
          "balance": {
            "raw_amount": "780000000",
            "amount": "780.000000",
            "decimals": 6
          },
          "quote": {
            "status": "available",
            "currency": "MXN",
            "unit_price": "1.00",
            "value": "780.000000",
            "price_as_of": "2026-06-16T15:03:59Z"
          }
        }
      ],
      "skipped": [],
      "errors": []
    }
  ],
  "errors": []
}
```

Bulk responses do not have one aggregate `observed_at`: each network call may
observe a different block and time. Evidence is therefore attached to each
account result. Accounts in the same network group share the same Bigwig
evidence block and observation time.

If Bigwig fails before establishing evidence for a network group, returns a
malformed success body, or is not called because every pair in the group was
skipped, affected account results use `"evidence": null`.

Mother may expose Bigwig block number, block hash, and `observed_at` as latest
pinned snapshot evidence. This evidence makes no finality claim. Mother must
not expose Bigwig `chain_id`, route, capability, authentication, or provider
internals.

## Balance and quote shaping

Bigwig returns only `raw_amount`. Mother calculates:

```txt
raw_amount + catalog decimals -> decimal amount string
amount + unit_price -> quote value string
```

Decimals, symbols, display names, and asset slugs come from Mother's catalog.
Quote prices and FX inputs come from Price Indexer.

Balances, prices, and quote values must use exact decimal or integer
arithmetic. Mother must not use floating-point arithmetic.

Quote status is one of:

```txt
available
unavailable
unsupported
```

`available` includes `currency`, `unit_price`, `value`, and `price_as_of`.
`unavailable` means a normally supported quote could not be resolved.
`unsupported` means no quote path exists for that asset and otherwise-supported
quote currency. For either non-available status, the position and resolved
balance remain present, quote value fields are `null`, and completeness is
`partial`.

## Status semantics

```txt
complete = all supported balances and quotes resolved
partial  = useful balance data exists, but a supported balance or quote failed
failed   = at least one supported balance item existed, but none resolved
```

Rules:

- unsupported asset-network pairs are skipped and do not affect completeness;
- all supported Bigwig items resolving with only unsupported pairs skipped is
  `complete`;
- mixed supported-item balance success and failure is `partial`;
- all supported Bigwig items failing is `failed`;
- a resolved balance with an `unavailable` or `unsupported` quote remains in
  `positions` and makes the affected account and response `partial`;
- a request with only skipped asset-network pairs remains `complete`;
- a Bigwig request-wide failure applies failure status to every supported item
  in that network group;
- bulk status aggregates account results: any degraded account makes the bulk
  response `partial` when another useful balance exists, and the bulk response
  is `failed` when no supported balance resolved anywhere.

Item errors stay inside the affected account result. Top-level `errors` are
reserved for failures that prevent orchestration of the request as a whole.

## Runtime error codes

Mother runtime codes are:

```txt
balance_provider_unavailable
price_provider_unavailable
balance_resolution_failed
price_resolution_failed
asset_network_map_unavailable
internal_error
```

## Skipped items

A skipped item means the asset slug is known globally but has no active,
supported representation on the account network.

```json
{
  "network_slug": "base-mainnet",
  "asset_slug": "bitso-mxn",
  "reason": "asset_not_supported_on_network"
}
```

Skipped pairs must be removed before constructing Bigwig targets.

## Non-goals for v1

Mother API does not:

- expose Bigwig route IDs or provider/runtime internals;
- allow callers to provide token contract addresses, decimals, or CAIP-2 IDs;
- pass Mother asset, catalog, quote, client, or Coto concepts into Bigwig;
- perform direct EVM JSON-RPC calls;
- infer assets from an address;
- auto-expand one address across multiple networks;
- resolve non-EVM or historical balances;
- discover accounts or assets automatically;
- return NFT balances, DeFi positions, or transaction history;
- own price, balance, holder, or event indexing;
- expose `asset_chain_map` or the internal `network` table directly;
- add in-process response caching, authentication, billing, rate limiting, or
  x402 boundaries.

## Future non-EVM support

The network-scoped model may later support non-EVM networks without changing
its meaning. Native Bitcoin and wrapped Bitcoin remain distinct canonical
assets and must not be silently collapsed.

## Recommended implementation sequence

### PR 1 - Spec alignment (complete)

- Accepted SPEC-006 and aligned it with the production Bigwig 3.5.0 contract.
- Locked canonical slugs, limits, grouping, target mapping, errors, and
  evidence.

### PR 2 - Catalog migration and target resolver (complete)

- Migrated `base`, `mantle`, and `arbitrum-one` catalog mappings to canonical
  `*-mainnet` slugs.
- Added batch-ready `(network_slug, asset_slug) -> BalanceTarget` resolution.
- Added native, ERC-20, unsupported-network, unsupported-asset, and
  unsupported-pair results with catalog-integrity validation.
- Kept Bigwig calls out of this slice.

### PR 3 - Bigwig client (complete)

- Added the authenticated client for
  `POST /internal/v1/primitives/evm/latest-balances`.
- Added request/response DTOs, error mapping, configuration/state wiring, and
  contract tests.
- Kept grouping, response correlation, catalog chain-ID checks, and public
  endpoint behavior out of this slice.

### PR 4 - Orchestration service (complete)

- Added the internal balance snapshot service with first-seen network grouping,
  caller-order restoration, catalog target resolution, defensive target
  deduplication, and unsupported-pair skips.
- Planned every network group before I/O, then called eligible Bigwig groups
  concurrently with at most one call per network.
- Added strict Bigwig evidence validation, catalog chain-ID checks,
  account-target correlation, pinned evidence retention, and internal item
  failure mapping.
- Kept decimal conversion, quote enrichment, public response shaping,
  validation, routes, and contract changes out of this slice.

### PR 5 - Quote and response shaping (complete)

- Added strict, deduplicated Price Indexer batch quote resolution while
  preserving unsupported, unavailable, provider-failure, and malformed
  outcomes.
- Added arbitrary-length integer and decimal-string arithmetic for catalog
  decimal conversion and exact quote multiplication without floating point.
- Added route-ready single and bulk response assembly for evidence, positions,
  skips, sanitized errors, summaries, and complete/partial/failed status.
- Kept public routes, request validation, and `CONTRACTS.md` changes out of
  this slice.

### PR 6 - Public endpoints and contracts (complete)

- Exposed `/v1/balances` and `/v1/balances/bulk` through the existing
  orchestration and response-assembly layers.
- Added JSON extraction, public validation, exact limits, canonical identifier
  admission, and deterministic request-wide error mapping.
- Added route-level success, degradation, validation, and contract coverage.
- Updated `CONTRACTS.md` and appended `HISTORY.md` in the same PR.

## Acceptance tests

Implementation acceptance tests must verify:

- Mother accepts canonical EVM network slugs that exist in the Mother catalog
  and are eligible for Bigwig latest-balance orchestration, including
  `eth-mainnet` when active;
- Mother rejects legacy slugs such as `base`, `mantle`, and `arbitrum-one`;
- Mother rejects non-EVM and unknown slugs for SPEC-006 v1;
- every Mother-accepted network slug is sent unchanged to Bigwig;
- `eth-mainnet` is not rejected by a hard-coded Base/Arbitrum allowlist; when
  catalog asset mappings exist and the Bigwig client returns a valid
  `eth-mainnet` evidence envelope, Mother returns a normal balance response;
- bulk requests are grouped by `network_slug`;
- caller account order is preserved within each network group;
- each non-empty network target group produces at most one Bigwig call and is
  never split into chunks;
- Bigwig receives only `network_slug`, `accounts[].address`, and concrete
  `targets`;
- Mother never sends `asset_slug`, symbols, decimals, quote currency,
  `client_ref`, Coto concepts, or `route_id` to Bigwig;
- native and ERC-20 assets map to their correct concrete target shapes;
- concrete targets are deduplicated before the Bigwig call while preserving
  first-occurrence order;
- requests above 50 accounts, 20 asset slugs, or 1,000 resolution items are
  rejected;
- every grouped Bigwig call remains within 50 accounts, 20 targets, and 1,000
  account-target items;
- unsupported asset-network pairs are skipped before calling Bigwig;
- skipped-only network groups do not call Bigwig and return null evidence;
- Bigwig responses are rejected as `internal_error` when primitive, network,
  chain ID, item cardinality, correlations, ordering, or status is inconsistent;
- Bigwig `observed_at`, block number, and block hash become account evidence;
- valid Bigwig `partial` and `failed` envelopes retain their pinned snapshot
  evidence;
- request-wide Bigwig failures and malformed success bodies produce null
  evidence;
- public evidence omits Bigwig `chain_id` and makes no finality claim;
- single-account `as_of.observed_at` equals its account evidence timestamp;
- bulk responses do not invent one aggregate observation time;
- Bigwig `raw_amount` is converted with Mother catalog decimals without
  floating-point arithmetic;
- Bigwig item failures become public item-level
  `balance_resolution_failed` errors without provider codes;
- Bigwig `404`, operation-resolution `422`, and `502 rpc_error` request-wide
  failures map to `balance_resolution_failed`;
- Bigwig authentication, rate-limit, availability, timeout, transport, and
  `500` failures map to `balance_provider_unavailable`;
- Bigwig validation errors after Mother admission and malformed or
  inconsistent success bodies map to `internal_error`;
- Bigwig `complete`, `partial`, and `failed` evidence maps to Mother status and
  errors;
- quote failure preserves the balance and produces `partial`;
- public responses never expose Bigwig route, provider, authentication, or
  runtime internals.
