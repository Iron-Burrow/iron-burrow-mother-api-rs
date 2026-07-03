---
status: active
owner: iron-burrow
last_reviewed: 2026-07-02
agent_edit_policy: update_when_relevant
---

# Private Beta API Quickstart

Use this quickstart when you have already received an Iron Burrow private Beta
API key and want to run your first balance and ERC-20 transfer queries.

The production base URL is:

```bash
export IB_API="https://api.ironburrow.com"
export IB_API_KEY="replace-with-issued-beta-key"
export AUTH_HEADER="Authorization: Bearer $IB_API_KEY"
```

Keep API keys server-side. Do not put keys in frontend code, public
repositories, logs, screenshots, browser extensions, client-side agents, or
shared chat transcripts.

The examples below use `curl` and `jq`. Replace placeholder account addresses
with the Ethereum or Base addresses you want to inspect.

## Health Check

The health endpoint is public and does not require an API key. It confirms that
the Mother API process is reachable.

```bash
curl -sS "$IB_API/health" | jq
```

## Single Balance Lookup

Use `/v1/balances` for one account on one network. The `network_slug` field is
the canonical network identifier; do not send `chain`, `chain_id`, or
`chain_slug`.

For balances, `as_of` currently supports only `{"kind": "latest"}`. Historical
balance snapshots, block-specific balances, and timestamp-specific balances
are not part of the private Beta surface yet.

```bash
curl -sS "$IB_API/v1/balances" \
  -H "$AUTH_HEADER" \
  -H "Content-Type: application/json" \
  -d '{
    "as_of": {
      "kind": "latest"
    },
    "account": {
      "network_slug": "eth-mainnet",
      "address": "0x1234567890abcdef1234567890abcdef1234beef",
      "client_ref": "main-wallet"
    },
    "quote_currency": "USD",
    "assets": [
      {
        "asset_slug": "ethereum"
      },
      {
        "asset_slug": "usdc"
      }
    ]
  }' | jq
```

Balance requests identify assets with canonical `asset_slug` values, such as
`ethereum` or `usdc`. They do not accept token contract addresses. If you only
know a token contract address today, use the ERC-20 transfer search endpoint's
`tokens.contract_addresses` filter, or ask Iron Burrow for the matching asset
slug before requesting balances.

## Bulk Balance Lookup

Use `/v1/balances/bulk` to query several explicit network-scoped accounts in
one request. Mother API does not infer networks or assets; each account and
asset must be requested directly.

```bash
curl -sS "$IB_API/v1/balances/bulk" \
  -H "$AUTH_HEADER" \
  -H "Content-Type: application/json" \
  -d '{
    "as_of": {
      "kind": "latest"
    },
    "accounts": [
      {
        "network_slug": "eth-mainnet",
        "address": "0x1234567890abcdef1234567890abcdef1234beef",
        "client_ref": "ethereum-wallet"
      },
      {
        "network_slug": "base-mainnet",
        "address": "0x2222222222222222222222222222222222222222",
        "client_ref": "base-wallet"
      }
    ],
    "quote_currency": "USD",
    "assets": [
      {
        "asset_slug": "ethereum"
      },
      {
        "asset_slug": "usdc"
      }
    ]
  }' | jq
```

## ERC-20 Transfers By Asset Slug

Use `/v1/erc20-transfers/search` for bounded ERC-20 `Transfer` logs on
Ethereum mainnet. Transfer search currently supports `eth-mainnet`.

Use `asset_slugs` when you want Iron Burrow to resolve a known catalog asset,
such as `usdc`, into its ERC-20 contract. Use `contract_addresses` when you
already know the token contract, or when the token is not yet in the Iron
Burrow catalog. You can also mix both filters in one request.

```bash
curl -sS "$IB_API/v1/erc20-transfers/search" \
  -H "$AUTH_HEADER" \
  -H "Content-Type: application/json" \
  -d '{
    "account": {
      "network_slug": "eth-mainnet",
      "address": "0xabc0000000000000000000000000000000000000",
      "client_ref": "treasury-main"
    },
    "direction": "any",
    "tokens": {
      "asset_slugs": [
        "usdc"
      ],
      "contract_addresses": []
    },
    "window": {
      "from_block": 18600000,
      "to_block": 18600500
    }
  }' | jq
```

The examples above use an explicit block window. Transfer search also accepts
these `window` alternatives:

```json
{
  "from_timestamp": "2026-06-25T00:00:00Z",
  "to_timestamp": "2026-06-25T01:00:00Z"
}
```

```json
{
  "lookback_seconds": 600,
  "to": "latest"
}
```

Use the `latest` lookback shape for simple recent activity checks. Keep
lookbacks within 86,400 seconds.

## ERC-20 Transfers By Contract

Use explicit contract addresses when you already know the token contract. This
example searches Ethereum mainnet USDC:
`0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48`.

```bash
curl -sS "$IB_API/v1/erc20-transfers/search" \
  -H "$AUTH_HEADER" \
  -H "Content-Type: application/json" \
  -d '{
    "account": {
      "network_slug": "eth-mainnet",
      "address": "0xabc0000000000000000000000000000000000000",
      "client_ref": "treasury-main"
    },
    "direction": "any",
    "tokens": {
      "asset_slugs": [],
      "contract_addresses": [
        "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
      ]
    },
    "window": {
      "from_block": 18600000,
      "to_block": 18600500
    }
  }' | jq
```

## Reading Responses

- Successful responses use `"ok": true`.
- Balance responses include `status`: `complete`, `partial`, or `failed`.
- Balance provider or quote failures can appear inside a `200 OK` response as
  item-level `errors`.
- Transfer responses include `limits.truncated`; `true` means the response is
  valid but capped by the row limit.
- Raw amounts, decimal amounts, prices, and quote values are strings to
  preserve precision. Do not parse them as floating point numbers.

## Important Limits

- Balance requests support up to 50 accounts, 20 assets, and 1,000
  account-asset resolution items.
- ERC-20 transfer search supports `eth-mainnet`, a 5,000-block inclusive
  window, 20 token filters, and 5,000 returned rows.
- ERC-20 asset slug filters must name ERC-20 tokens on the requested network.
  Native `ethereum` is not silently converted to WETH.

## Common Issues

| HTTP status | Error code | What it usually means |
| ----------- | ---------- | --------------------- |
| `401` | `unauthorized` | The API key is missing, invalid, revoked, expired, or not active. |
| `429` | `rate_limited` | The API key exceeded its configured request limit. |
| `400` | `invalid_request` | The JSON body is malformed, missing a required field, or uses an unsupported reserved field. |
| `400` | `unknown_field` | The request includes a field that is not part of the Beta API contract. |
| `400` | `unsupported_network` | A balance request used an unknown, legacy, or non-canonical network slug. |
| `404` | `unsupported_network` | A transfer search request used a network other than `eth-mainnet`. |
| `422` | `window_too_large` | The ERC-20 transfer search window exceeds the public limit. |

For fields beyond this quickstart, ask Iron Burrow for the current API
contract notes.
