---
status: draft
owner: iron-burrow
last_reviewed: 2026-06-25
agent_edit_policy: update_when_relevant
---

# SPEC-007 - Public ERC-20 Transfer Search v1

Draft implementation spec for a future public Mother API ERC-20 transfer
search endpoint.

This draft does not change the current public contract. `/v1/erc20-transfers/search`
must not be treated as exposed or promised until the endpoint is implemented
and [CONTRACTS.md](../../CONTRACTS.md) and [HISTORY.md](../../HISTORY.md) are
updated in the same change.

Before this draft can be accepted, it must cite the binding Bigwig
transfer-extraction contract and version, including the internal endpoint path,
request and response shapes, limits, and error taxonomy that Mother API will
consume.

## Purpose

This endpoint gives callers a bounded, synchronous search for ERC-20 `Transfer`
logs involving one watched EVM address. The caller may search all ERC-20 token
contracts in the bounded window or restrict the search with:

- Mother API `asset_slug` filters resolved through the canonical catalog;
- explicit ERC-20 `contract_addresses`.

The endpoint does not create jobs, store artifacts, expose persisted cursors,
or perform background indexing.

## Ownership

Mother API owns:

- the public route, request DTOs, response DTOs, and error envelope;
- public validation for `network_slug`, address, direction, window, and token
  filters;
- `asset_slug` plus `network_slug` resolution through the Mother-owned catalog;
- normalization, merging, deduplication, and public token-filter limits;
- response shaping and catalog enrichment;
- mapping Bigwig runtime failures into Mother API public errors.

Bigwig owns:

- bounded ERC-20 transfer extraction;
- internal operation-capable route resolution;
- provider access, including `eth_getLogs`;
- block, timestamp, lookback, row, chunk, timeout, and finality protections;
- raw transfer evidence returned to Mother API.

Mother API must not perform direct EVM JSON-RPC calls, event indexing, holder
indexing, price lookup, fiat valuation, native-to-wrapped asset conversion, or
provider-specific extraction logic for this endpoint.

## Endpoint

```http
POST /v1/erc20-transfers/search
```

## Request

```json
{
  "network_slug": "eth-mainnet",
  "address": "0xabc0000000000000000000000000000000000000",
  "direction": "any",
  "tokens": {
    "asset_slugs": ["usdc", "usdt"],
    "contract_addresses": [
      "0x1111111111111111111111111111111111111111"
    ]
  },
  "window": {
    "from_block": 18600000,
    "to_block": 18600500
  }
}
```

## Request Fields

### `network_slug`

Required string.

Canonical Mother API network identifier, for example:

```json
"network_slug": "eth-mainnet"
```

Mother API validates the network against its supported catalog and configured
Bigwig transfer-extraction integration. Bigwig remains responsible for
operation-capable route resolution.

### `address`

Required string.

The watched wallet address. It must be a concrete `0x` 20-byte EVM address.
This is not the ERC-20 token contract address.

### `direction`

Required string.

Allowed values:

```text
any | from | to
```

Meaning:

- `from`: transfers where `address` is the ERC-20 sender.
- `to`: transfers where `address` is the ERC-20 recipient.
- `any`: transfers where `address` is either sender or recipient.

### `tokens`

Optional object.

When omitted, `null`, or empty, the endpoint searches all ERC-20 transfers
involving the watched wallet within the bounded window.

```json
"tokens": null
```

```json
"tokens": {
  "asset_slugs": [],
  "contract_addresses": []
}
```

Both forms mean no token-contract filter.

#### `tokens.asset_slugs`

Optional array of canonical Mother API asset slugs.

```json
"asset_slugs": ["usdc", "usdt"]
```

Each `asset_slug` must resolve, for the requested `network_slug`, to an active
ERC-20 representation with a concrete `0x` 20-byte contract address.

Mother API rejects asset slugs that:

- do not exist in the Mother API asset catalog;
- exist globally but are not available on the requested network;
- exist on the requested network but represent a native asset;
- exist on the requested network but are not represented by an ERC-20 contract;
- are configured as ERC-20-compatible but have no concrete contract address.

Mother API must never silently convert native assets into wrapped assets:

```text
ethereum != wrapped-ether
ETH != WETH
```

If the caller wants WETH transfer logs, the caller must request the wrapped
asset slug, such as `wrapped-ether`, or provide the WETH contract address.

#### `tokens.contract_addresses`

Optional array of concrete ERC-20 token contract addresses.

```json
"contract_addresses": [
  "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
]
```

Each entry must be a concrete `0x` 20-byte EVM address. Mother API normalizes
accepted contract addresses to lowercase.

### `window`

Required object.

Exactly one supported bounded window form must be provided.

Block range example:

```json
"window": {
  "from_block": 18600000,
  "to_block": 18600500
}
```

Timestamp range example:

```json
"window": {
  "from_timestamp": "2026-06-25T00:00:00Z",
  "to_timestamp": "2026-06-25T01:00:00Z"
}
```

Lookback example:

```json
"window": {
  "lookback_blocks": 500
}
```

Mother API performs public-facing validation before calling Bigwig. Bigwig's
accepted window semantics and hard extraction limits must come from the cited
Bigwig transfer-extraction contract before this draft is accepted.

## Token Filter Resolution

Mother API resolves the final Bigwig `contract_addresses` set before any
Bigwig call:

1. Validate `network_slug`, watched wallet `address`, `direction`, and
   `window`.
2. Validate `tokens` shape, asset-slug formats, and explicit contract-address
   formats.
3. Resolve every `tokens.asset_slugs[]` entry into a concrete ERC-20 contract
   address for the requested `network_slug`.
4. Reject the whole request if any asset slug is invalid or incompatible.
5. Normalize all explicit and resolved contract addresses to lowercase.
6. Merge explicit contract addresses and resolved asset contract addresses.
7. Deduplicate the merged set.
8. Enforce Mother API's public max token filter limit.
9. Pass the merged set to Bigwig as `contract_addresses`.
10. If the merged set is empty, omit or pass an empty `contract_addresses`
    filter according to the cited Bigwig contract's unfiltered-search shape.

Mother API must never pass `asset_slug` values to Bigwig.

## Internal Bigwig Request

Mother API calls Bigwig's internal transfer-extraction endpoint with concrete
fields only.

Illustrative shape:

```json
{
  "network_slug": "eth-mainnet",
  "address": "0xabc0000000000000000000000000000000000000",
  "direction": "any",
  "contract_addresses": [
    "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
    "0xdac17f958d2ee523a2206206994597c13d831ec7",
    "0x1111111111111111111111111111111111111111"
  ],
  "window": {
    "from_block": 18600000,
    "to_block": 18600500
  }
}
```

The final path, auth model, limit fields, and response/error shape must be
filled in from the binding Bigwig transfer-extraction contract before
acceptance.

## Response

```json
{
  "ok": true,
  "type": "erc20_transfer_search",
  "network_slug": "eth-mainnet",
  "address": "0xabc0000000000000000000000000000000000000",
  "direction": "any",
  "window": {
    "from_block": 18600000,
    "to_block": 18600500
  },
  "token_filters": {
    "requested": {
      "asset_slugs": ["usdc", "usdt"],
      "contract_addresses": [
        "0x1111111111111111111111111111111111111111"
      ]
    },
    "resolved_contract_addresses": [
      {
        "contract_address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
        "asset_slug": "usdc",
        "symbol": "USDC",
        "decimals": 6,
        "source": "asset_slug"
      },
      {
        "contract_address": "0xdac17f958d2ee523a2206206994597c13d831ec7",
        "asset_slug": "usdt",
        "symbol": "USDT",
        "decimals": 6,
        "source": "asset_slug"
      },
      {
        "contract_address": "0x1111111111111111111111111111111111111111",
        "asset_slug": null,
        "symbol": null,
        "decimals": null,
        "source": "contract_address"
      }
    ]
  },
  "transfers": [
    {
      "block_number": 18600001,
      "tx_hash": "0x0000000000000000000000000000000000000000000000000000000000000000",
      "log_index": 12,
      "token": {
        "contract_address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
        "asset_slug": "usdc",
        "symbol": "USDC",
        "decimals": 6
      },
      "from": "0xabc0000000000000000000000000000000000000",
      "to": "0xdef0000000000000000000000000000000000000",
      "amount": {
        "raw": "12500000",
        "decimal": "12.5"
      },
      "direction": "from"
    }
  ],
  "limits": {
    "truncated": false,
    "max_rows": 1000
  }
}
```

## Response Field Notes

### `token_filters.requested`

Echoes the public token filters after basic normalization.

### `token_filters.resolved_contract_addresses`

Shows the actual token-contract set used for the internal Bigwig call.
Customers need to know exactly what Mother API searched.

### `transfers[].token.contract_address`

The ERC-20 token contract that emitted the `Transfer` log. This value comes
from Bigwig raw transfer evidence.

### `transfers[].token.asset_slug`

Included only when Mother API can map the emitting token contract back to a
known catalog asset on the requested network. For unknown explicit contract
addresses, this field is `null`.

### `amount.raw`

Required string.

The raw ERC-20 transfer value from the log, represented as a base-unit integer
string.

### `amount.decimal`

Optional string.

Included only when Mother API knows the token decimals from its catalog. For
unknown explicit contract addresses, this field is `null` or omitted.

### `limits.truncated`

Boolean.

Indicates whether internal Bigwig row limits prevented the endpoint from
returning all matching rows. If `true`, the client should retry with a narrower
window.

## Validation Rules

The request must reject unknown fields. Mother API validates public input and
catalog resolution before calling Bigwig whenever possible.

| Condition                                         | HTTP status | `error.code`                         |
| ------------------------------------------------- | ----------: | ------------------------------------ |
| Malformed JSON                                    |         400 | `invalid_json`                       |
| Unknown request field                             |         400 | `unknown_field`                      |
| Missing `network_slug`                            |         400 | `missing_network_slug`               |
| Unsupported network                               |         404 | `unsupported_network`                |
| Missing or malformed wallet address               |         400 | `invalid_address`                    |
| Invalid direction                                 |         400 | `invalid_direction`                  |
| Missing or malformed window                       |         400 | `invalid_window`                     |
| Window exceeds public limit                       |         422 | `window_too_large`                   |
| Invalid asset slug format                         |         400 | `invalid_asset_slug`                 |
| Unknown asset slug                                |         404 | `asset_not_found`                    |
| Asset not available on requested network          |         422 | `asset_not_available_on_network`     |
| Asset is not an ERC-20 token on requested network |         422 | `asset_not_erc20_on_network`         |
| ERC-20 asset mapping lacks a contract address     |         503 | `asset_contract_mapping_unavailable` |
| Malformed contract address                        |         400 | `invalid_contract_address`           |
| Too many unique token filters                     |         422 | `too_many_token_filters`             |
| Bigwig extraction disabled or unavailable         |         503 | `extraction_unavailable`             |
| Provider failure                                  |         502 | `upstream_provider_error`            |
| Provider timeout                                  |         504 | `upstream_provider_timeout`          |

Asset-slug validation is request-wide. If a mixed request includes valid
contract addresses and one invalid or incompatible asset slug, Mother API
rejects the whole request and does not call Bigwig.

Unknown asset slugs, native assets, and non-ERC-20 assets must never produce an
empty successful transfer response. Returning an empty transfer list for a bad
asset filter would incorrectly suggest that the query was valid and no logs
matched.

## Error Shape

This draft uses the current Mother API error envelope shape. Structured
`details` are deliberately deferred unless the shared error contract is updated
in a future accepted implementation.

Unknown asset slug example:

```json
{
  "ok": false,
  "error": {
    "code": "asset_not_found",
    "message": "Asset was not found."
  }
}
```

Asset unavailable on the requested network example:

```json
{
  "ok": false,
  "error": {
    "code": "asset_not_available_on_network",
    "message": "Asset is not available on the requested network."
  }
}
```

Native or non-ERC-20 asset example:

```json
{
  "ok": false,
  "error": {
    "code": "asset_not_erc20_on_network",
    "message": "Asset is not an ERC-20 token on the requested network."
  }
}
```

Catalog mapping unavailable example:

```json
{
  "ok": false,
  "error": {
    "code": "asset_contract_mapping_unavailable",
    "message": "Asset contract mapping is temporarily unavailable."
  }
}
```

Too many token filters example:

```json
{
  "ok": false,
  "error": {
    "code": "too_many_token_filters",
    "message": "Too many token filters were requested."
  }
}
```

## Public Limit

Add a Mother API public limit:

```yaml
erc20_transfers:
  max_token_filters: 20
```

This limit must be less than or equal to the Bigwig internal max contract
address filter count declared by the binding transfer-extraction contract.
Mother API should fail at startup if its public limit exceeds the configured
Bigwig limit.

## DTO Names

Recommended Rust DTO names:

```rust
Erc20TransferSearchRequest
Erc20TransferSearchResponse
Erc20TransferSearchWindow
Erc20TransferTokenFilters
ResolvedErc20TokenFilter
Erc20TransferRow
Erc20TransferToken
Erc20TransferAmount
```

All public DTOs should derive the serialization and schema traits used by
Mother API's existing public route DTOs.

## Non-Goals

This endpoint does not add:

- `/transactions`;
- generic transaction history;
- native token transfers;
- NFT transfers;
- swaps;
- prices;
- fiat valuation;
- customer aliases;
- background jobs;
- persisted cursors;
- long-range historical indexing;
- automatic on-chain metadata discovery for arbitrary contracts;
- inbound API keys, billing, rate limiting, or x402 boundaries.

Unknown contract addresses are allowed as filters, but they are not
automatically promoted into catalog assets.

## Acceptance Criteria

This draft is implementation-ready when:

- the binding Bigwig transfer-extraction contract and version are cited;
- `asset_slug` filters resolve into concrete ERC-20 contract addresses for the
  requested network;
- explicit `contract_addresses` are accepted;
- mixed asset-slug and contract-address filters are accepted;
- invalid asset slugs and invalid contract addresses produce distinct errors;
- unknown asset slugs return `asset_not_found`;
- native assets such as `ethereum` on `eth-mainnet` return
  `asset_not_erc20_on_network`;
- assets known globally but unavailable on the requested network return
  `asset_not_available_on_network`;
- ERC-20-compatible catalog mappings without concrete addresses return
  `asset_contract_mapping_unavailable`;
- Mother API never silently maps native assets to wrapped assets;
- resolved and explicit contract addresses are normalized and deduplicated;
- too many unique resolved token contracts returns `too_many_token_filters`;
- empty, omitted, or null token filters preserve unfiltered ERC-20 transfer
  search behavior;
- invalid asset slugs in mixed requests reject the whole request;
- Mother API does not call Bigwig when asset resolution fails;
- Mother API never passes asset slugs to Bigwig;
- Bigwig receives only normalized concrete `contract_addresses`;
- response includes the resolved contract-address set used for the search;
- response enriches known token contracts with catalog metadata;
- unknown explicit token contracts remain usable but have nullable catalog
  metadata;
- public documentation includes examples for asset-slug filters,
  contract-address filters, mixed filters, and unfiltered searches;
- when the route is implemented, [CONTRACTS.md](../../CONTRACTS.md) documents
  the public endpoint, response shape, limits, and error responses in the same
  change.
