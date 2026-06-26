---
status: accepted
owner: iron-burrow
last_reviewed: 2026-06-25
agent_edit_policy: update_when_relevant
external_contract:
  - iron-burrow-infra-gateway/CONTRACTS.md@3.5.2
  - iron-burrow-infra-gateway/docs/specs/SPEC-005-bigwig-hub-extractor-module.md@accepted
  - iron-burrow-infra-gateway/docs/specs/SPEC-008-bigwig-internal-erc20-transfer-contract-address-filters.md@accepted
---

# SPEC-007 - Public ERC-20 Transfer Search v1

Accepted implementation spec for a future public Mother API ERC-20 transfer
search endpoint.

This spec does not change the current public contract. `/v1/erc20-transfers/search`
must not be treated as exposed or promised until the endpoint is implemented
and [CONTRACTS.md](../../CONTRACTS.md) and [HISTORY.md](../../HISTORY.md) are
updated in the same change.

The internal Bigwig transfer-extraction dependency is binding as of
`iron-burrow-infra-gateway` 3.5.2. Mother API consumes Bigwig's Hub-only,
authenticated `POST /internal/v1/extractions/erc20-transfers` contract as
documented in Bigwig `CONTRACTS.md` and accepted Bigwig SPEC-005/SPEC-008.
Those sources define the internal endpoint path, request and response shapes,
limits, timeout behavior, all-or-nothing result behavior, and error taxonomy
that this spec uses.

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
  "lookback_seconds": 600,
  "to": "latest"
}
```

Mother API performs public-facing validation before calling Bigwig. Bigwig's
accepted window semantics are block range, timestamp range, or
`lookback_seconds` with `to: "latest"`. Mother API must shape its internal
Bigwig request to one of those forms.

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
6. Merge resolved asset contract addresses first, then explicit contract
   addresses.
7. Deduplicate the merged set.
8. Enforce Mother API's public max token filter limit.
9. Pass the merged set to Bigwig as `contract_addresses`.
10. If the merged set is empty, omit or pass an empty `contract_addresses`
    filter; Bigwig treats omitted, `null`, and `[]` as no token-contract
    filter.

`max_token_filters` counts the final unique concrete contract-address set
after asset-slug resolution, normalization, merge, and deduplication. Mother
API does not enforce a separate raw pre-deduplication token-filter count
limit, though malformed JSON, invalid slug formats, invalid address formats,
and unknown fields are still rejected before catalog lookup.

Mother API must never pass `asset_slug` values to Bigwig.

## Internal Bigwig Contract

Mother API calls Bigwig's implemented internal transfer-extraction endpoint:

```http
POST /internal/v1/extractions/erc20-transfers
```

The endpoint is Hub-only, authenticated, and registered only when Bigwig Hub
configuration has `extraction.enabled: true`. Mother API must authenticate
with the configured Bigwig bearer token and should identify itself with
Bigwig's internal client-service header convention when the client layer is
implemented.

### Bigwig Request

Mother API sends concrete fields only:

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

Bigwig request fields are:

| Field                | Type                    | Required | Notes |
| -------------------- | ----------------------- | -------- | ----- |
| `network_slug`       | string                  | Yes      | As of Bigwig 3.5.2 ERC-20 extraction supports `eth-mainnet`. |
| `address`            | string                  | Yes      | `0x` plus 40 hex characters; Bigwig normalizes to lowercase. |
| `direction`          | string                  | No       | `from`, `to`, or `any`; Bigwig defaults to `any`. |
| `contract_addresses` | array of string \| null | No       | Concrete ERC-20 token contract addresses. Omitted, `null`, or `[]` means no token-contract filter. |
| `window`             | object                  | Yes      | Exactly one supported Bigwig window shape. |

Supported Bigwig windows:

```json
{"from_block": 18600000, "to_block": 18600500}
```

```json
{
  "from_timestamp": "2026-06-25T00:00:00Z",
  "to_timestamp": "2026-06-25T01:00:00Z"
}
```

```json
{"lookback_seconds": 600, "to": "latest"}
```

Bigwig rejects unknown outer fields and unknown window fields. Bigwig does not
accept Mother API `asset_slug`, symbol, decimals, quote, pagination, or public
response-shaping fields.

### Bigwig Response

Bigwig returns raw ERC-20 transfer evidence:

```json
{
  "extractor": "evm_erc20_transfers_by_address",
  "network_slug": "eth-mainnet",
  "address": "0xabc0000000000000000000000000000000000000",
  "direction": "any",
  "window_kind": "block",
  "from_block": 18600000,
  "to_block": 18600500,
  "latest_block": 18600500,
  "safe_block": 18600488,
  "finality": {
    "status": "mixed",
    "safe_block": 18600488,
    "latest_block": 18600500,
    "reorg_risk": true,
    "policy": "confirmation_lag",
    "confirmation_lag": 12
  },
  "rows_extracted": 1,
  "results": [
    {
      "block_number": 18600001,
      "tx_hash": "0x0000000000000000000000000000000000000000000000000000000000000001",
      "log_index": 12,
      "token": "0x1111111111111111111111111111111111111111",
      "from": "0xabc0000000000000000000000000000000000000",
      "to": "0x2222222222222222222222222222222222222222",
      "value": "1000000000000000000"
    }
  ]
}
```

`window_kind` is `block`, `timestamp`, or `lookback`. `finality.status` is
`finalized`, `mixed`, or `unfinalized`; mixed and unfinalized responses are
allowed and set `reorg_risk: true`. `rows_extracted` always equals
`results.length`. Rows are sorted by block number, log index, then transaction
hash.

Mother API must validate successful Bigwig responses before public shaping:

- `extractor` is `evm_erc20_transfers_by_address`;
- `network_slug`, normalized `address`, and `direction` match the request;
- returned block bounds and `window_kind` are coherent with the requested
  window;
- `safe_block` and `latest_block` match the nested `finality` fields;
- `rows_extracted` equals `results.length`;
- each result row has valid block, transaction hash, log index, token, sender,
  recipient, and raw value fields.

### Bigwig Limits

Bigwig 3.5.2 production extraction limits are:

| Limit | Current value | Notes |
| ----- | ------------- | ----- |
| Max block range | derived | `eth_getLogs.max_block_span * extraction.max_chunks`; current `eth-mainnet` is `500 * 10 = 5,000` inclusive blocks. |
| Max lookback | `86400` seconds | `lookback_seconds` must be positive and at or below this value. |
| Max rows | `5000` | Bigwig rejects oversized results as `result_too_large`. |
| Max contract addresses | `20` | Applied after Bigwig normalizes and deduplicates `contract_addresses`. |
| Overall timeout | `30` seconds | Bigwig returns `extraction_timeout` for the whole synchronous operation deadline. |
| Per-provider timeout | route-defined | Bigwig returns `provider_timeout` for one protected upstream operation. |

Bigwig's row behavior is all-or-nothing. It never returns a successful
response containing only part of the matching rows for this extractor. If
unique rows would exceed `max_rows`, Bigwig stops scanning, discards
accumulated rows, and returns `422 result_too_large`.

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
    "max_rows": 5000
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

### `limits.max_rows`

Integer.

Indicates the Bigwig row limit used for the search. Bigwig extraction is
all-or-nothing: if the search exceeds this limit, Mother API maps Bigwig
`result_too_large` to an error instead of returning partial rows.

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
| Bigwig result row limit exceeded                  |         422 | `result_too_large`                   |
| Bigwig extraction disabled or unavailable         |         503 | `extraction_unavailable`             |
| Bigwig extraction deadline exceeded               |         504 | `extraction_timeout`                 |
| Provider failure                                  |         502 | `upstream_provider_error`            |
| Provider timeout                                  |         504 | `upstream_provider_timeout`          |
| Malformed Bigwig success or impossible validation |         500 | `internal_error`                     |

Asset-slug validation is request-wide. If a mixed request includes valid
contract addresses and one invalid or incompatible asset slug, Mother API
rejects the whole request and does not call Bigwig.

Unknown asset slugs, native assets, and non-ERC-20 assets must never produce an
empty successful transfer response. Returning an empty transfer list for a bad
asset filter would incorrectly suggest that the query was valid and no logs
matched.

## Bigwig Error Taxonomy

Mother API must treat Bigwig errors as internal dependency outcomes and return
sanitized Mother API errors. It must not expose Bigwig route IDs, provider
names, upstream URLs, authentication details, capability versions, or raw
provider diagnostics.

| Bigwig status/code | Mother result |
| --- | --- |
| `400 invalid_extraction_request` after Mother shaping | `500 internal_error` |
| `400 invalid_address` after Mother validation | `500 internal_error` |
| `400 invalid_contract_address` after Mother validation | `500 internal_error` |
| `400 invalid_direction` after Mother validation | `500 internal_error` |
| `400 invalid_window_shape` after Mother validation | `500 internal_error` |
| `401 unauthorized` | `503 extraction_unavailable`; never expose authentication details |
| `404 unsupported_network` after Mother admission | `503 extraction_unavailable` |
| `422 reversed_block_range` | `400 invalid_window` |
| `422 block_out_of_range` | `400 invalid_window` |
| `422 reversed_timestamp_range` | `400 invalid_window` |
| `422 timestamp_out_of_range` | `400 invalid_window` |
| `422 lookback_too_large` | `422 window_too_large` |
| `422 range_too_large` | `422 window_too_large` |
| `422 too_many_contract_addresses` after Mother limit enforcement | `500 internal_error` |
| `422 result_too_large` | `422 result_too_large` |
| `429 gateway_rate_limited` | `503 extraction_unavailable`; retain `Retry-After` internally |
| `502 rpc_error` | `502 upstream_provider_error` |
| `503 provider_unavailable` | `503 extraction_unavailable`; retain `Retry-After` internally |
| `504 provider_timeout` | `504 upstream_provider_timeout` |
| `504 extraction_timeout` | `504 extraction_timeout` |
| `500 internal_error` | `503 extraction_unavailable` |
| Transport failure or Mother client timeout | `503 extraction_unavailable` or `504 upstream_provider_timeout`, according to the failure class |

Bigwig malformed success bodies, unexpected success statuses, malformed error
responses, or unknown Bigwig error codes are Mother API `internal_error`
conditions.

## Error Shape

This accepted spec uses the current Mother API error envelope shape. Structured
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

This limit equals the Bigwig 3.5.2 production
`extraction.max_contract_addresses` value. Mother API should fail at startup
if its public limit exceeds the configured Bigwig limit.

The limit is enforced after Mother API resolves asset slugs, normalizes all
explicit and resolved contract addresses, merges the two sources, and
deduplicates by concrete contract address. Duplicate raw filter entries do not
count multiple times after deduplication.

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

This accepted spec is implementation-ready for code when:

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
