---
status: draft
owner: iron-burrow
last_reviewed: 2026-06-16
agent_edit_policy: update_when_relevant
external_contract: iron-burrow-price-indexer/CONTRACTS.md@2026-06-02
---

# SPEC-006 - Network-Scoped Balances v1

Draft proposal for Mother API endpoints that resolve latest balances for one
or more network-scoped addresses across canonical Iron Burrow assets.

This document is not a public contract. It remains non-binding until accepted,
implemented, and reflected in [CONTRACTS.md](../../CONTRACTS.md). The
implementation PR that adds these public endpoints must update
[CONTRACTS.md](../../CONTRACTS.md) and [HISTORY.md](../../HISTORY.md) in the
same change.

## Purpose

This spec proposes a compact balance snapshot surface for Coto / Control Tower
dashboard views. A caller can request balances for explicit addresses on
explicit networks and receive latest balances with quote values in one
response.

The endpoint is multi-network only in this narrow sense: one request may
include accounts from more than one supported network. It must not imply that
a bare address applies to every network, and Mother API must not create
implicit address-network combinations.

V1 is limited to EVM latest balances.

## Core definitions

### Network-scoped address

A balance subject is the pair:

```txt
(network_slug, address)
```

This matters because an EOA may share the same address across EVM networks,
while a Safe, smart wallet, or contract account may exist on only one network,
may not exist on another, or may represent a different deployment.

Callers must explicitly provide each network-scoped address they want resolved.

### Network slug

`network_slug` is the public Mother API identifier for a supported network.
Mother API may internally store CAIP-2 values or provider metadata, but callers
use the Mother API slug.

V1 examples from the current seeded EVM catalog:

```txt
eth-mainnet
arbitrum-one
base
mantle
```

`bitcoin-mainnet` exists in the current catalog, but non-EVM balance resolution
is out of scope for v1.

### Canonical asset slug

`asset_slug` is the canonical network-agnostic asset identifier from
`mother_api.global_asset.slug`.

Examples from the current seeded catalog:

```txt
usdc
bitso-mxn
ethereum
wrapped-ether
wrapped-bitcoin
bitcoin
```

Symbols and aliases are not request slugs. `MXNB`, `WETH`, `WBTC`, and `BTC`
may be display symbols or aliases, but requests must use the canonical slugs
above.

### Asset-network map

Mother API owns the catalog mapping:

```txt
(asset_slug, network_slug) -> network-specific asset metadata
```

The internal map may include token contract address, decimals, native-asset
metadata, provider configuration, aliases, pricing metadata, and CAIP-2
references. Callers must not provide or duplicate this mapping.

## Ownership

Coto / Control Tower owns the watched-account model:

```txt
network_slug + address + label/tags/client_ref
```

Mother API owns public routing, catalog lookup, validation, latest balance
orchestration, and response shaping for this proposed surface.

Mother API must not own or reimplement:

- price indexing, price derivation, or historical price data;
- balance indexing or holder indexing;
- direct EVM node I/O;
- DeFi protocol position math;
- protocol-specific reserve lookup;
- automatic address discovery;
- in-process response caching;
- auth, billing, rate limiting, or x402 boundaries.

## Balance provider boundary

Mother API owns latest balance orchestration for this endpoint.

For v1, Mother API must not perform direct EVM node I/O. Raw latest EVM balance
evidence must come from Bigwig through an accepted internal Bigwig
balance-evidence primitive.

The provider split is:

```txt
Mother API resolves:
  network_slug + asset_slug -> native/ERC-20 balance target

Bigwig resolves:
  route/network + account + native/ERC-20 target -> raw_amount evidence

Price Indexer resolves:
  asset quote price, FX, and quote value inputs
```

SPEC-006 cannot be accepted until the Bigwig balance-evidence primitive is
accepted and documented. Quote prices come from the
`iron-burrow-price-indexer` Query Layer.

## Proposed endpoints

Single network-scoped address:

```http
POST /v1/balances
```

Bulk network-scoped addresses:

```http
POST /v1/balances/bulk
```

These routes are proposed only. They are not part of the current public
contract until implemented and added to `CONTRACTS.md`.

## V1 scope

Supported in v1:

- EVM network-scoped addresses only.
- Latest balances only.
- Caller-supplied account lists only.
- Caller-supplied canonical asset slugs only.
- Quote values for supported quote currencies.

Supported `as_of` request:

```json
{
  "kind": "latest"
}
```

Reserved for future use:

```json
{
  "kind": "timestamp",
  "timestamp": "2026-06-16T15:00:00Z"
}
```

Historical requests must fail validation with `unsupported_as_of` until
explicitly implemented.

## Request model

Single account request:

```json
{
  "as_of": {
    "kind": "latest"
  },
  "account": {
    "network_slug": "arbitrum-one",
    "address": "0x1234567890abcdef1234567890abcdef1234beef",
    "client_ref": "main-safe-arbitrum"
  },
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

Bulk request:

```json
{
  "as_of": {
    "kind": "latest"
  },
  "accounts": [
    {
      "network_slug": "eth-mainnet",
      "address": "0x1234567890abcdef1234567890abcdef1234beef",
      "client_ref": "main-safe-mainnet"
    },
    {
      "network_slug": "arbitrum-one",
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

Resolution is the cartesian product of the provided accounts and assets:

```txt
network_scoped_addresses[] x assets[]
```

For the single-account endpoint:

```txt
1 account x assets[]
```

Mother API must resolve only the explicit pairs in the request.

## Response model

Single response:

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
    "network_slug": "arbitrum-one",
    "address": "0x1234567890abcdef1234567890abcdef1234beef",
    "client_ref": "main-safe-arbitrum"
  },
  "positions": [
    {
      "network_slug": "arbitrum-one",
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
    },
    {
      "network_slug": "arbitrum-one",
      "asset_slug": "bitso-mxn",
      "symbol": "MXNB",
      "balance": {
        "raw_amount": "1250000000",
        "amount": "1250.000000",
        "decimals": 6
      },
      "quote": {
        "status": "available",
        "currency": "MXN",
        "unit_price": "1.00",
        "value": "1250.000000",
        "price_as_of": "2026-06-16T15:03:59Z"
      }
    }
  ],
  "skipped": [],
  "errors": []
}
```

Bulk response with an unsupported asset-network pair skipped:

```json
{
  "ok": true,
  "type": "balances_bulk",
  "status": "complete",
  "as_of": {
    "kind": "latest",
    "observed_at": "2026-06-16T15:04:30Z"
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
        "network_slug": "eth-mainnet",
        "address": "0x1234567890abcdef1234567890abcdef1234beef",
        "client_ref": "main-safe-mainnet"
      },
      "positions": [
        {
          "network_slug": "eth-mainnet",
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
          "network_slug": "eth-mainnet",
          "asset_slug": "bitso-mxn",
          "reason": "asset_not_supported_on_network"
        }
      ],
      "errors": []
    },
    {
      "status": "complete",
      "account": {
        "network_slug": "arbitrum-one",
        "address": "0xabcdefabcdefabcdefabcdefabcdefabcdefabcd",
        "client_ref": "ops-wallet-arbitrum"
      },
      "positions": [
        {
          "network_slug": "arbitrum-one",
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
          "network_slug": "arbitrum-one",
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

Decimal values must be returned as JSON strings. Mother API must not parse
prices, balances, or quote values into floating-point values.

## Status semantics

```txt
complete = all supported items resolved successfully
partial  = some supported items resolved, but at least one supported item failed
failed   = no useful balance data could be returned
```

Unsupported asset-network combinations are skipped, not failed. Skipped items
do not make the response `partial`.

## Skipped items

A skipped item is not a provider failure. It means the asset slug is known
globally, but no supported active representation exists for the account
network in v1.

Example:

```json
{
  "network_slug": "eth-mainnet",
  "asset_slug": "bitso-mxn",
  "reason": "asset_not_supported_on_network"
}
```

`bitso-mxn` is currently seeded on `arbitrum-one`, so requesting it for
`eth-mainnet` should be skipped unless a future migration adds an active
`eth-mainnet` mapping.

## Validation rules

The request must fail validation when:

- `as_of.kind` is unsupported;
- an address is invalid for the requested network family;
- a network slug is unknown or unsupported for v1 balances;
- an asset slug is unknown globally;
- the quote currency is unsupported;
- `assets` is empty;
- `accounts` is empty in bulk;
- duplicate network-scoped addresses are present;
- duplicate asset slugs are present;
- request limits are exceeded.

Recommended validation error codes:

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

For EVM networks, duplicate account detection should compare addresses
case-insensitively after validating that they are 20-byte hex addresses.
Responses should echo the caller-provided address casing.

## Runtime error codes

Recommended runtime error codes:

```txt
balance_provider_unavailable
price_provider_unavailable
balance_resolution_failed
price_resolution_failed
asset_network_map_unavailable
```

Runtime failures for supported resolution items may produce `partial` or
`failed` responses. The implementation should keep item-level errors inside
the affected account response and reserve top-level `errors` for request-wide
runtime failures.

## Request limits

Mother API must enforce hard limits.

Suggested v1 limits:

```txt
max_accounts: 50
max_assets: 25
max_resolution_items: 1000
```

Where:

```txt
resolution_items = accounts.length x assets.length
```

For the single endpoint:

```txt
resolution_items = 1 x assets.length
```

## Non-goals for v1

Out of scope:

- non-EVM address resolution;
- historical balances;
- automatic address-network expansion;
- automatic asset discovery;
- NFT balances;
- DeFi protocol positions;
- transaction history;
- caller-provided token contracts;
- caller-provided token decimals;
- caller-provided CAIP-2 identifiers;
- exposing `asset_chain_map` directly;
- exposing Mother API's internal `network` table structure;
- in-process response caching;
- API-key context middleware, auth, billing, rate limiting, or x402.

## Future non-EVM support

The core model should support non-EVM networks later without changing the
meaning of a network-scoped address.

For example, native Bitcoin could be represented as:

```json
{
  "network_slug": "bitcoin-mainnet",
  "address": "bc1q...",
  "client_ref": "btc-cold-wallet-1"
}
```

With:

```json
{
  "asset_slug": "bitcoin"
}
```

Mother API would then resolve:

```txt
bitcoin-mainnet + bitcoin -> native Bitcoin balance resolver
```

Wrapped Bitcoin on EVM networks remains a distinct canonical asset:

```txt
wrapped-bitcoin
```

Native Bitcoin and wrapped Bitcoin must not be silently collapsed into the
same asset slug.

## Implementation notes

The first implementation may use a mock balance repository, but it should keep
the final provider boundary intact:

```txt
Caller provides: network-scoped addresses + asset_slugs
Mother API resolves: asset_slug + network_slug through the catalog
Mother API returns: latest balances and quote values
```

Acceptance tests for an implementation PR should cover:

- single `arbitrum-one` request for `usdc` and `bitso-mxn`;
- bulk `eth-mainnet` plus `arbitrum-one` request where `bitso-mxn` is skipped
  on `eth-mainnet`;
- rejection of unknown asset slugs and unsupported network slugs;
- rejection of duplicate accounts and duplicate assets;
- rejection of non-`latest` `as_of` requests;
- `partial` behavior when a supported balance or quote item fails;
- limit enforcement for accounts, assets, and resolution items.
