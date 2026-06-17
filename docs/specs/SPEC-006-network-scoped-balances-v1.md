---
status: draft
owner: iron-burrow
last_reviewed: 2026-06-17
agent_edit_policy: update_when_relevant
external_contracts:
  - iron-burrow-price-indexer/CONTRACTS.md@2026-06-02
  - iron-burrow-infra-gateway/CONTRACTS.md@v3.5
---

# SPEC-006 - Network-Scoped Balances v1

Draft proposal for Mother API endpoints that resolve latest balances for one
or more network-scoped EVM addresses across canonical Iron Burrow assets.

This document is not a public contract. It remains non-binding until accepted,
implemented, and reflected in [CONTRACTS.md](../../CONTRACTS.md). The
implementation PR that adds these public endpoints must update
[CONTRACTS.md](../../CONTRACTS.md) and [HISTORY.md](../../HISTORY.md) in the
same change.

The Bigwig EVM latest balance evidence primitive is available in production as
of Bigwig v3.5. Mother API SPEC-006 plans against that contract.

## Purpose

This spec proposes a compact balance snapshot surface for Coto / Control Tower.
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

Mother API and Bigwig must share one canonical Iron Burrow `network_slug`
namespace:

```txt
eth-mainnet
base-mainnet
mantle-mainnet
arbitrum-mainnet
```

For the initial v1 rollout, Mother API enables balances only for:

```txt
base-mainnet
arbitrum-mainnet
```

Bigwig v3.5 has operation-capable latest-balance routes for those two networks.
`eth-mainnet` and `mantle-mainnet` remain canonical catalog identifiers but
must return `unsupported_network` for this endpoint until Bigwig has an
operation-capable route for them.

Mother's current seeded catalog contains legacy values including `base`,
`mantle`, and `arbitrum-one`. Before enabling either public endpoint, the
implementation must migrate those catalog values and their asset mappings to
`base-mainnet`, `mantle-mainnet`, and `arbitrum-mainnet`.

Legacy slugs are not balance aliases in v1. Requests using them must return
`unsupported_network`. Mother must not translate public slugs into Bigwig route
IDs, and must never send or receive `route_id`.

A request accepted by Mother must be callable against Bigwig v3.5 using the
same `network_slug`.

`bitcoin-mainnet` may remain in the catalog, but non-EVM balance resolution is
out of scope for v1.

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
  network_slug + account address + concrete target -> raw_amount evidence

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

## Proposed endpoints

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
BigwigLatestBalancesClient
PriceQuoteClient
BalanceResponseAssembler
```

These routes are proposed only. They are not part of the public contract until
implemented and added to `CONTRACTS.md`.

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
Bigwig v3.5 limits:

```txt
max_accounts: 50
max_targets: 20
max_account_target_items: 1000
```

Mother returns `request_too_large` if the public request or any grouped Bigwig
call would exceed its applicable limits.

## Bigwig integration

### Bigwig v3.5 adapter

Bigwig accepts exactly one network per request. Mother must group accounts by
`network_slug`.

For each group, Mother must:

1. collect the requested accounts for that network;
2. resolve each requested `asset_slug` to an active network-specific target;
3. skip unsupported asset-network pairs before calling Bigwig;
4. deduplicate concrete targets;
5. call `POST /internal/v1/primitives/evm/latest-balances`;
6. map evidence items back to Mother positions using catalog metadata.

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
`kind:erc20 + lower(contract_address)`.

### Bigwig error mapping

Request-wide Bigwig failures map as follows:

| Bigwig failure | Mother result |
| --- | --- |
| `401 unauthorized` | `balance_provider_unavailable`; never expose authentication details |
| `404 unsupported_network` | `unsupported_network` only if Mother missed validation; otherwise `balance_resolution_failed` |
| `422 network_not_enabled_for_operation` | `balance_resolution_failed` |
| `422 no_route_satisfies_operation` | `balance_resolution_failed` |
| `429 gateway_rate_limited` | `balance_provider_unavailable`; retain `Retry-After` internally |
| `502 rpc_error` | `balance_resolution_failed` |
| `503 provider_unavailable` | `balance_provider_unavailable` |
| `504 provider_timeout` | `balance_provider_unavailable` |
| `500 internal_error` | `balance_provider_unavailable` |

An unexpected Bigwig request-validation error after Mother accepted and shaped
the request is a Mother `internal_error`.

A request-wide Bigwig failure affects every supported account-target item in
that network group, and each affected pair receives the mapped Mother error
code in its account result. Other network groups may still succeed, allowing a
bulk response to be `partial`.

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

If Bigwig fails before establishing evidence for a network group, affected
account results use `"evidence": null`.

Mother may expose Bigwig block number, block hash, and `observed_at` as balance
evidence. It must not expose Bigwig route or provider internals.

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

### PR 1 - Spec alignment

- Align SPEC-006 with the production Bigwig v3.5 contract.
- Lock canonical slugs, limits, grouping, target mapping, errors, and evidence.

### PR 2 - Catalog migration and target resolver

- Migrate `base`, `mantle`, and `arbitrum-one` catalog mappings to canonical
  `*-mainnet` slugs.
- Add `(network_slug, asset_slug) -> BalanceTarget`.
- Support native, ERC-20, and unsupported-pair results.
- Do not call Bigwig yet.

### PR 3 - Bigwig client

- Add the authenticated client for
  `POST /internal/v1/primitives/evm/latest-balances`.
- Add request/response DTOs, error mapping, and contract tests.

### PR 4 - Orchestration service

- Group accounts by network, resolve and deduplicate targets, skip unsupported
  pairs, call Bigwig, and retain response correlations.

### PR 5 - Quote and response shaping

- Apply catalog metadata and exact decimal conversion.
- Resolve quote values through Price Indexer.
- Assemble evidence, positions, skips, errors, and status.

### PR 6 - Public endpoints and contracts

- Expose `/v1/balances` and `/v1/balances/bulk`.
- Add validation and limit enforcement.
- Update `CONTRACTS.md` and append `HISTORY.md` in the same PR.

## Acceptance tests

Implementation acceptance tests must verify:

- Mother accepts canonical `base-mainnet` and `arbitrum-mainnet` and rejects
  legacy balance slugs;
- every Mother-accepted network slug is sent unchanged to Bigwig;
- bulk requests are grouped by `network_slug`;
- Bigwig receives only `network_slug`, `accounts[].address`, and concrete
  `targets`;
- Mother never sends `asset_slug`, symbols, decimals, quote currency,
  `client_ref`, Coto concepts, or `route_id` to Bigwig;
- native and ERC-20 assets map to their correct concrete target shapes;
- concrete targets are deduplicated before the Bigwig call;
- requests above 20 asset slugs or 1,000 resolution items are rejected;
- every grouped Bigwig call remains within 50 accounts, 20 targets, and 1,000
  account-target items;
- unsupported asset-network pairs are skipped before calling Bigwig;
- Bigwig `observed_at`, block number, and block hash become account evidence;
- bulk responses do not invent one aggregate observation time;
- Bigwig `raw_amount` is converted with Mother catalog decimals without
  floating-point arithmetic;
- Bigwig item failures become public item-level
  `balance_resolution_failed` errors without provider codes;
- Bigwig `complete`, `partial`, `failed`, and request-wide failures map to
  Mother status and errors;
- quote failure preserves the balance and produces `partial`;
- public responses never expose Bigwig route, provider, authentication, or
  runtime internals.
