---
status: contract
owner: iron-burrow
last_reviewed: 2026-06-29
agent_edit_policy: update_only_if_contract_changes
---

# CONTRACTS.md

This file is the binding public surface of `iron-burrow-mother-api-rs`
(Mother API). It describes the HTTP endpoints, request inputs, response
shapes, and error envelopes that Mother API promises to expose. Anything
not documented here is not a promise, regardless of what the running code
happens to return today.

Mother API is currently in Production Alpha 1. The contract surface is
intentionally minimal. The old TypeScript gateway sprawl (admin, explorer,
account, tracked-token, price, auth, billing, x402) is explicitly **not**
part of this contract and will not be reintroduced without an accepted RFC
or spec.

This contract evolves with [HISTORY.md](HISTORY.md). Behavior changes that
affect any field below require a coordinated update to both files in the
same change.

## Conventions

- Base URL in production is `https://api.ironburrow.com`.
- All responses are JSON. `Content-Type: application/json`.
- All timestamps are ISO-8601 with explicit UTC offset (`Z` or
  `+00:00`).
- Success envelopes set `"ok": true`. Error envelopes set `"ok": false`
  and include a stable `error.code`.
- Unknown query parameters are ignored.
- Response object key order is not contractual. Field **presence** and
  **type** are contractual.
- Decimal-typed values (prices, ratios) are returned as JSON **strings**
  to preserve precision. Clients must not parse them as floats.

## API lifecycle

Mother API uses these lifecycle states:

- **Stable**: part of the current supported public contract.
- **Experimental**: public but subject to contract changes before promotion
  to Stable.
- **Deprecated**: retained temporarily for compatibility, receives no new
  features, and has a documented removal target.
- **Removed**: no longer exposed at runtime; retained in project history only.
- **Internal**: service-to-service behavior that is not a public API promise.

Unless an endpoint is explicitly labeled otherwise, documented public
endpoints are Stable.

## Stable endpoints

| Method | Path                                    | Auth | Notes                                                       |
| ------ | --------------------------------------- | ---- | ----------------------------------------------------------- |
| `GET`  | `/health`                               | None | Dependency-free liveness probe.                             |
| `GET`  | `/v1/status`                            | None | Informational readiness with dependency checks.             |
| `GET`  | `/v1/assets`                            | None | Lists active global assets with optional price enrichment.  |
| `GET`  | `/v1/assets/{slug}`                     | None | Returns one active asset, its asset network maps, and a price block. |
| `GET`  | `/v1/assets/{slug}/signal/price-stats`  | None | Returns a strict price statistics signal for one asset.      |
| `GET`  | `/v1/assets/{slug}/signal/price-trend`  | None | Returns a strict price trend signal for one asset.           |
| `POST` | `/v1/balances`                          | None | Resolves one latest network-scoped EVM balance snapshot.     |
| `POST` | `/v1/balances/bulk`                     | None | Resolves latest snapshots for explicit network accounts.     |
| `POST` | `/v1/erc20-transfers/search`            | None | Feature-gated by `ERC20_TRANSFERS_ENABLED`; searches bounded ERC-20 Transfer logs for one EVM address. |
| `GET`  | `/v1/search-engine`                     | None | Resolves a search query against global assets.     |

Mother API does not currently authenticate callers. API keys, rate
limiting, billing, and x402 are explicitly out of scope.

---

### `GET /health`

Dependency-free liveness probe. Does not read from the database, the
price-indexer Query Layer, or any other dependency. Always returns
`200 OK` when the process is up.

**Response — `200 OK`:**

```json
{
  "ok": true,
  "service": "iron-burrow-mother-api",
  "mascot": "Capitan Sousa",
  "message": "Happy squirrel, systems nominal."
}
```

Fields:

| Field     | Type   | Notes                                          |
| --------- | ------ | ---------------------------------------------- |
| `ok`      | bool   | Always `true`.                                 |
| `service` | string | Always `"iron-burrow-mother-api"`.             |
| `mascot`  | string | Always `"Capitan Sousa"`.                      |
| `message` | string | Stable human-readable status line.             |

---

### `GET /v1/status`

Informational readiness endpoint. Exposes dependency-check results.
Returns `200 OK` regardless of dependency state; readiness is reflected
in `ok` and `checks`.

**Response — `200 OK`:**

```json
{
  "ok": true,
  "service": "iron-burrow-mother-api",
  "version": "0.1.1",
  "environment": "production",
  "mascot": "Capitan Sousa",
  "message": "Mother API is online.",
  "checks": {
    "app": "ok",
    "database": "reachable",
    "price_indexer": "configured",
    "dis": "configured",
    "evm_indexer": "not_connected"
  }
}
```

Fields:

| Field             | Type    | Notes                                                                              |
| ----------------- | ------- | ---------------------------------------------------------------------------------- |
| `ok`              | bool    | `true` when no required check is failing.                                          |
| `service`         | string  | Always `"iron-burrow-mother-api"`.                                                 |
| `version`         | string  | Cargo package version of the running binary.                                       |
| `environment`     | string  | Runtime label from `APP_ENV` (e.g., `"development"`, `"production"`).              |
| `mascot`          | string  | Always `"Capitan Sousa"`.                                                          |
| `message`         | string  | Stable human-readable status line.                                                 |
| `checks.app`      | string  | Always `"ok"`.                                                                     |
| `checks.database` | string  | One of `"reachable"`, `"unreachable"`, `"skipped"`. `"skipped"` when unconfigured. |
| `checks.price_indexer` | string | One of `"configured"`, `"not_configured"`, `"invalid_config"`. Config/client availability only; not a live price-indexer network probe. |
| `checks.dis`      | string  | One of `"configured"`, `"not_configured"`, `"invalid_config"`. Config/client availability only; not a live DIS network probe. |
| `checks.evm_indexer`   | string | Currently always `"not_connected"`. Reserved.                                    |

`ok` is `false` when `checks.database` is `"unreachable"`. `"skipped"` is
treated as healthy because the database is optional.

---

### `GET /v1/assets`

Lists active Mother API-owned global assets, with optional USD price
enrichment from the internal price-indexer Query Layer.

**Query parameters:**

| Name    | Type    | Required | Default | Notes                                                      |
| ------- | ------- | -------- | ------- | ---------------------------------------------------------- |
| `limit` | integer | No       | `100`   | Positive integer. Clamped to a maximum of `1000`.          |

**Response — `200 OK`:**

```json
{
  "ok": true,
  "type": "assets",
  "limit": 100,
  "count": 21,
  "assets": [
    {
      "asset_id": "bitcoin",
      "symbol": "BTC",
      "name": "Bitcoin",
      "category": "crypto",
      "canonical_path": "/assets/bitcoin",
      "price": {
        "status": "available",
        "price": "2500.123456",
        "quote_currency": "USD",
        "source_type": "chainlink",
        "confidence_label": null,
        "is_fallback": false,
        "is_derived": false,
        "recorded_at": "2026-05-20T12:00:01.000Z",
        "warning": null
      }
    }
  ]
}
```

Top-level fields:

| Field    | Type    | Notes                                                       |
| -------- | ------- | ----------------------------------------------------------- |
| `ok`     | bool    | Always `true` on success.                                   |
| `type`   | string  | Always `"assets"`.                                          |
| `limit`  | integer | Effective limit after clamping.                             |
| `count`  | integer | Number of entries returned in `assets`.                     |
| `assets` | array   | Asset list items (see below).                               |

Each asset list item:

| Field            | Type   | Notes                                                                     |
| ---------------- | ------ | ------------------------------------------------------------------------- |
| `asset_id`       | string | Stable asset slug (e.g., `"bitcoin"`).                                    |
| `symbol`         | string | Asset symbol (e.g., `"BTC"`).                                             |
| `name`           | string | Display name.                                                             |
| `category`       | string | Asset category (e.g., `"crypto"`, `"commodity"`).                         |
| `canonical_path` | string | Public canonical path (e.g., `"/assets/bitcoin"`).                        |
| `price`          | object | Price block (see [Price block](#price-block)). Always present.            |

**Errors:**

- `400 invalid_limit` — `limit` is not a positive integer or is `0`.
- `503 database_unavailable` — `DATABASE_URL` is unset or Postgres is
  unreachable.

### `GET /v1/assets/{slug}`

Returns one active asset, the network-specific asset network maps the UI can use
to render asset detail pages, and a stable price block. Optional price
signals can be requested for one-call asset-page rendering.

**Path parameters:**

| Name   | Type   | Required | Notes                                                          |
| ------ | ------ | -------- | -------------------------------------------------------------- |
| `slug` | string | Yes      | Asset slug. Compared case-insensitively after trimming.        |

**Query parameters:**

| Name            | Type   | Required | Default | Allowed values | Notes |
| --------------- | ------ | -------- | ------- | -------------- | ----- |
| `include`       | string | No       | none    | `priceStats`, `priceTrend`, `priceSeries` | Comma-separated. Tokens are trimmed and matched case-insensitively. Unknown tokens are ignored. |
| `quoteCurrency` | string | No       | `USD`   | `USD`, `MXN`, `USDC`, `BTC` | Trimmed and uppercased. Applies to the latest `price` block and all requested enrichments. Empty or unsupported values return `400 invalid_request`. |
| `window`        | string | No       | `24h`   | `1h`, `24h`, `7d`, `30d` | Applies only when at least one known enrichment is requested. |
| `granularity`   | string | No       | upstream default | `5m`, `1h`, `1d` | Forwarded only when provided and only for requested enrichments. |

Allowed `window` and `granularity` combinations for requested enrichments:

| `window` | Allowed granularities |
| -------- | --------------------- |
| `1h`     | `5m`                  |
| `24h`    | `5m`, `1h`            |
| `7d`     | `1h`                  |
| `30d`    | `1d`                  |

Mother API does not expose `asOf` on asset detail and never forwards legacy
parameters such as `range`, `resolution`, `from`, `to`, `interval`,
`sourceType`, `limit`, or `beforeId` to price-indexer.

The latest-price lookup always forwards the normalized `quoteCurrency` to
price-indexer. This allows price-indexer-owned direct or derived prices to
populate the base `price` block without Mother API performing conversion.

**Response — `200 OK`, full enrichment happy path:**

```json
{
  "ok": true,
  "type": "asset",
  "asset": {
    "asset_id": "usdc",
    "symbol": "USDC",
    "name": "USD Coin",
    "category": "crypto",
    "canonical_path": "/assets/usdc"
  },
  "price": {
    "status": "available",
    "price": "1.0001",
    "quote_currency": "USD",
    "source_type": "coingecko",
    "confidence_label": "high",
    "is_fallback": false,
    "is_derived": false,
    "recorded_at": "2026-05-26T12:00:05Z",
    "warning": null
  },
  "asset_network_maps": [
    {
      "network_slug": "eth-mainnet",
      "network_name": "Ethereum Mainnet",
      "caip2": "eip155:1",
      "is_native": false,
      "address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
    }
  ],
  "signals": {
    "price_stats": {
      "slug": "usdc",
      "quoteCurrency": "USD",
      "window": "24h",
      "granularity": "1h",
      "sampleCount": 24,
      "percentChange": "0.000100",
      "warnings": []
    },
    "price_trend": {
      "slug": "usdc",
      "quoteCurrency": "USD",
      "window": "24h",
      "granularity": "1h",
      "direction": "flat",
      "confidence": "high",
      "warnings": []
    },
    "price_series": {
      "points": [
        {
          "bucketStart": "2026-06-02T11:00:00.000Z",
          "price": "1.0001",
          "status": "observed"
        }
      ],
      "meta": {
        "expectedBucketCount": 24,
        "sampleCount": 1
      }
    }
  },
  "enrichment_errors": []
}
```

**Response — `200 OK`, partial enrichment failure:**

```json
{
  "ok": true,
  "type": "asset",
  "asset": {
    "asset_id": "usdc",
    "symbol": "USDC",
    "name": "USD Coin",
    "category": "crypto",
    "canonical_path": "/assets/usdc"
  },
  "price": {
    "status": "available",
    "price": "1.0001",
    "quote_currency": "USD",
    "source_type": "coingecko",
    "confidence_label": "high",
    "is_fallback": false,
    "is_derived": false,
    "recorded_at": "2026-05-26T12:00:05Z",
    "warning": null
  },
  "asset_network_maps": [
    {
      "network_slug": "eth-mainnet",
      "network_name": "Ethereum Mainnet",
      "caip2": "eip155:1",
      "is_native": false,
      "address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
    }
  ],
  "signals": {
    "price_stats": {
      "slug": "usdc",
      "quoteCurrency": "USD",
      "window": "24h",
      "granularity": "1h",
      "warnings": []
    },
    "price_trend": null
  },
  "enrichment_errors": [
    {
      "source": "price_trend",
      "code": "signal_not_available",
      "message": "Price trend is not available."
    }
  ]
}
```

**Response — `200 OK`, price-indexer disabled or unavailable:**

```json
{
  "ok": true,
  "type": "asset",
  "asset": {
    "asset_id": "usdc",
    "symbol": "USDC",
    "name": "USD Coin",
    "category": "crypto",
    "canonical_path": "/assets/usdc"
  },
  "price": {
    "status": "unavailable",
    "price": null,
    "quote_currency": null,
    "source_type": null,
    "confidence_label": null,
    "is_fallback": false,
    "is_derived": false,
    "recorded_at": null,
    "warning": null
  },
  "asset_network_maps": [
    {
      "network_slug": "eth-mainnet",
      "network_name": "Ethereum Mainnet",
      "caip2": "eip155:1",
      "is_native": false,
      "address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
    }
  ]
}
```

Top-level fields:

| Field        | Type   | Notes                                                       |
| ------------ | ------ | ----------------------------------------------------------- |
| `ok`         | bool   | Always `true` on success.                                   |
| `type`       | string | Always `"asset"`.                                           |
| `asset`      | object | Asset summary (see below).                                  |
| `price`      | object | Price block (see [Price block](#price-block)). Always present. |
| `asset_network_maps` | array | Asset network map entries (see below). May be empty. |
| `signals`    | object | Present only when at least one known enrichment is requested. Contains only requested enrichment keys. |
| `enrichment_errors` | array | Present only when at least one known enrichment is requested. Empty when all requested enrichments succeed. |

Asset summary:

| Field            | Type   | Notes                                              |
| ---------------- | ------ | -------------------------------------------------- |
| `asset_id`       | string | Stable asset slug.                                 |
| `symbol`         | string | Asset symbol.                                      |
| `name`           | string | Display name.                                      |
| `category`       | string | Asset category.                                    |
| `canonical_path` | string | Public canonical path.                             |

Asset network map entry:

| Field             | Type            | Notes                                                                  |
| ----------------- | --------------- | ---------------------------------------------------------------------- |
| `network_slug`    | string          | Canonical network slug (e.g., `"eth-mainnet"`).                        |
| `network_name`    | string          | Network display name.                                                  |
| `caip2`           | string \| null  | CAIP-2 identifier when known.                                          |
| `is_native`       | bool            | `true` when the asset is the network's native asset.                   |
| `address`         | string \| null  | Token contract address. `null` for native assets or when not applicable. |

EVM asset network maps use canonical Iron Burrow mainnet slugs, including
`base-mainnet`, `mantle-mainnet`, and `arbitrum-mainnet`. The legacy catalog
values `base`, `mantle`, and `arbitrum-one` are not emitted.

Optional `signals` fields:

| Field          | Type          | Notes |
| -------------- | ------------- | ----- |
| `price_stats`  | object \| null | Present only when `include` contains `priceStats`. Successful values are pass-through price-indexer stats JSON. |
| `price_trend`  | object \| null | Present only when `include` contains `priceTrend`. Successful values are pass-through price-indexer trend JSON. |
| `price_series` | object \| null | Present only when `include` contains `priceSeries`. Successful values are pass-through price-indexer series JSON, including `points` and `meta`. |

Successful signal payloads preserve upstream fields and `warnings` exactly.
Warnings are not failures and do not create `enrichment_errors` entries.
Decimal values remain JSON strings.

Each enrichment error entry:

| Field     | Type   | Notes |
| --------- | ------ | ----- |
| `source`  | string | One of `price_stats`, `price_trend`, `price_series`. |
| `code`    | string | Stable Mother API enrichment error code. |
| `message` | string | Mother API-owned public message. Upstream messages are not propagated. |

Enrichment error codes:

| Code                         | Notes |
| ---------------------------- | ----- |
| `invalid_request`            | Enrichment query parameters are unsupported or the `window`/`granularity` combination is invalid. |
| `signal_not_available`       | Price-indexer reported the requested signal is not available. |
| `upstream_auth_failed`       | Mother API could not authenticate to price-indexer. |
| `price_indexer_error`        | Price-indexer failed while handling a valid enrichment request. |
| `price_indexer_unavailable`  | Price-indexer is unconfigured, unreachable, or timed out. |
| `upstream_invalid_response`  | Price-indexer returned malformed JSON or an unexpected error envelope. |

When the base asset exists, enrichment failures do not fail the endpoint.
The requested signal value is `null`, one `enrichment_errors` entry is added,
and the response remains `200 OK`. Invalid enrichment parameters follow this
partial-response rule.

**Errors:**

- `400 invalid_request` — `quoteCurrency` is empty or unsupported.
- `404 asset_not_found` — No active asset exists for the given slug.
- `503 database_unavailable` — `DATABASE_URL` is unset or Postgres is
  unreachable.

---

### `GET /v1/assets/{slug}/signal/price-stats`

Returns one strict price statistics signal from the internal
price-indexer Query Layer. This endpoint does not read Mother API's asset
database and does not calculate statistics locally.

**Path parameters:**

| Name   | Type   | Required | Notes                              |
| ------ | ------ | -------- | ---------------------------------- |
| `slug` | string | Yes      | Asset slug forwarded as `slug`.    |

**Query parameters:**

| Name            | Type   | Required | Default | Allowed values              | Notes |
| --------------- | ------ | -------- | ------- | --------------------------- | ----- |
| `quoteCurrency` | string | No       | `USD`   | `USD`, `MXN`, `USDC`, `BTC` | Trimmed and uppercased before forwarding. |
| `window`        | string | No       | `24h`   | `1h`, `24h`, `7d`, `30d`   | Uses the current window model. |
| `granularity`   | string | No       | upstream default | `5m`, `1h`, `1d` | Forwarded only when provided. |

Allowed `window` and `granularity` combinations:

| `window` | Allowed granularities |
| -------- | --------------------- |
| `1h`     | `5m`                  |
| `24h`    | `5m`, `1h`            |
| `7d`     | `1h`                  |
| `30d`    | `1d`                  |

Mother API does not expose `asOf` and never forwards legacy parameters
such as `range` or `resolution`.

**Response — `200 OK`:**

```json
{
  "ok": true,
  "type": "price_stats",
  "signal": {
    "slug": "ethereum",
    "assetId": "00000000-0000-0000-0000-000000000001",
    "quoteCurrency": "USD",
    "window": "24h",
    "granularity": "1h",
    "from": "2026-06-01T11:00:00.000Z",
    "to": "2026-06-02T11:00:00.000Z",
    "expectedBucketCount": 24,
    "sampleCount": 20,
    "carryForwardBucketCount": 2,
    "missingBucketCount": 2,
    "coverageRatio": "0.833333",
    "firstPrice": "3812.45",
    "lastPrice": "3890.10",
    "minPrice": "3812.45",
    "maxPrice": "3890.10",
    "meanPrice": "3845.55",
    "medianPrice": "3840.00",
    "sampleStdDev": "12.340000",
    "coefficientOfVariation": "0.003210",
    "absoluteChange": "77.65",
    "percentChange": "0.020367",
    "minTimestamp": "2026-06-01T13:00:00.000Z",
    "maxTimestamp": "2026-06-02T10:00:00.000Z",
    "warnings": ["low_series_coverage"]
  }
}
```

Top-level fields:

| Field    | Type   | Notes                                                       |
| -------- | ------ | ----------------------------------------------------------- |
| `ok`     | bool   | Always `true` on success.                                   |
| `type`   | string | Always `"price_stats"`.                                     |
| `signal` | object | Price-indexer statistics object. Upstream fields and warnings are preserved. |

The price-indexer Query Layer owns the `signal` fields, decimal string
formatting, bucket counts, coverage, warnings, and future informational
field additions.

**Errors:**

- `400 invalid_request` — Query parameters are unsupported or the
  `window`/`granularity` combination is invalid.
- `404 asset_not_found` — The asset or requested price series is not
  available for this signal.
- `502 upstream_auth_failed` — Mother API could not authenticate to the
  price-indexer Query Layer.
- `502 price_indexer_error` — Price-indexer failed while handling a valid
  request.
- `502 upstream_invalid_response` — Price-indexer returned malformed JSON
  or an unexpected error envelope.
- `503 price_indexer_unavailable` — Price-indexer is unconfigured,
  unreachable, or timed out.

---

### `GET /v1/assets/{slug}/signal/price-trend`

Returns one strict price trend signal from the internal price-indexer
Query Layer. Parameters, validation, ownership, and error behavior match
`GET /v1/assets/{slug}/signal/price-stats`.

**Response — `200 OK`:**

```json
{
  "ok": true,
  "type": "price_trend",
  "signal": {
    "slug": "ethereum",
    "assetId": "00000000-0000-0000-0000-000000000001",
    "quoteCurrency": "USD",
    "window": "24h",
    "granularity": "1h",
    "from": "2026-06-01T11:00:00.000Z",
    "to": "2026-06-02T11:00:00.000Z",
    "expectedBucketCount": 24,
    "sampleCount": 20,
    "carryForwardBucketCount": 2,
    "missingBucketCount": 2,
    "coverageRatio": "0.833333",
    "firstPrice": "3812.45",
    "lastPrice": "3890.10",
    "percentChange": "0.020367",
    "direction": "up",
    "slope": "0.000812",
    "slopeUnit": "per_hour",
    "rSquared": "0.640000",
    "confidence": "medium",
    "warnings": ["missing_buckets_detected"]
  }
}
```

Top-level fields:

| Field    | Type   | Notes                                                       |
| -------- | ------ | ----------------------------------------------------------- |
| `ok`     | bool   | Always `true` on success.                                   |
| `type`   | string | Always `"price_trend"`.                                     |
| `signal` | object | Price-indexer trend object. Upstream fields and warnings are preserved. |

The price-indexer Query Layer owns the `signal` fields, trend direction,
confidence, warnings, and future informational field additions.

---

### `POST /v1/balances`

Resolves the latest balances for one explicitly network-scoped EVM account
across the requested canonical assets. The endpoint accepts JSON only.

The account's `network_slug` and each `asset_slug` are exact canonical
identifiers. They are not trimmed or case-normalized. The address must be
exactly `0x` followed by 40 ASCII hexadecimal characters; EIP-55 checksum
validation is not required. The response preserves caller-provided address
casing and `client_ref`.

Unknown JSON fields are ignored except for reserved network alias fields.
Requests containing `chain`, `chain_id`, or `chain_slug` at the top level,
under `account`, or under `accounts[]` return `400 invalid_request`. Missing
required fields, wrong field types, malformed JSON, or a missing/non-JSON
`Content-Type` also return `400 invalid_request`.

**Request:**

```json
{
  "as_of": {
    "kind": "latest"
  },
  "account": {
    "network_slug": "eth-mainnet",
    "address": "0x1234567890abcdef1234567890abcdef1234beef",
    "client_ref": "main-safe"
  },
  "quote_currency": "MXN",
  "assets": [
    {
      "asset_slug": "ethereum"
    }
  ]
}
```

Request fields:

| Field | Type | Required | Notes |
| ----- | ---- | -------- | ----- |
| `as_of.kind` | string | Yes | Must be exactly `"latest"`. |
| `account.network_slug` | string | Yes | Exact canonical active EVM network slug. Legacy slugs are unsupported. |
| `account.address` | string | Yes | `0x` plus 40 ASCII hex characters. |
| `account.client_ref` | string | No | Opaque caller reference, echoed unchanged; `null` when omitted. |
| `quote_currency` | string | Yes | Trimmed and uppercased; allowed values are `USD`, `MXN`, `USDC`, and `BTC`. |
| `assets` | array | Yes | One to 20 unique canonical asset entries. |
| `assets[].asset_slug` | string | Yes | Exact canonical global asset slug. Symbols and aliases are not accepted. |

**Response — `200 OK`:**

```json
{
  "ok": true,
  "type": "balances",
  "status": "complete",
  "as_of": {
    "kind": "latest",
    "observed_at": "2026-06-18T12:00:00Z"
  },
  "quote_currency": "MXN",
  "account": {
    "network_slug": "eth-mainnet",
    "address": "0x1234567890abcdef1234567890abcdef1234beef",
    "client_ref": "main-safe"
  },
  "evidence": {
    "source": "bigwig",
    "network_slug": "eth-mainnet",
    "block": {
      "number": "22900000",
      "hash": "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    },
    "observed_at": "2026-06-18T12:00:00Z"
  },
  "positions": [
    {
      "network_slug": "eth-mainnet",
      "asset_slug": "ethereum",
      "symbol": "ETH",
      "balance": {
        "raw_amount": "1000000000000000000",
        "amount": "1.000000000000000000",
        "decimals": 18
      },
      "quote": {
        "status": "available",
        "currency": "MXN",
        "unit_price": "35000.50",
        "value": "35000.500000000000000000",
        "price_as_of": "2026-06-18T11:59:59Z"
      }
    }
  ],
  "skipped": [],
  "errors": []
}
```

For this endpoint, `as_of.observed_at` equals `evidence.observed_at`. Both are
`null` when no Bigwig evidence was established.

---

### `POST /v1/balances/bulk`

Uses the same validation and resolution model as `/v1/balances`, but accepts
one to 50 explicit network-scoped accounts. Resolution is the cartesian
product `accounts[] x assets[]`; Mother API does not infer assets or expand an
address to unrequested networks.

**Request:**

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
    }
  ],
  "quote_currency": "USD",
  "assets": [
    {
      "asset_slug": "usdc"
    }
  ]
}
```

Accounts are unique by `(network_slug, lowercase(address))`. The same address
on different networks is allowed. Assets are unique by exact `asset_slug`.
Caller account order and asset order are preserved in the response.

Public limits are enforced before orchestration:

| Limit | Maximum |
| ----- | ------- |
| Accounts | 50 |
| Assets | 20 |
| Account-asset resolution items | 1,000 |

Requests are rejected rather than split when a public or grouped Bigwig limit
would be exceeded.

**Response — `200 OK`:**

```json
{
  "ok": true,
  "type": "balances_bulk",
  "status": "complete",
  "as_of": {
    "kind": "latest"
  },
  "quote_currency": "USD",
  "summary": {
    "requested_accounts": 1,
    "requested_assets": 1,
    "requested_resolution_items": 1,
    "positions_returned": 1,
    "skipped_items": 0,
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
          "number": "32000000",
          "hash": "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        },
        "observed_at": "2026-06-18T12:00:00Z"
      },
      "positions": [
        {
          "network_slug": "base-mainnet",
          "asset_slug": "usdc",
          "symbol": "USDC",
          "balance": {
            "raw_amount": "1250000",
            "amount": "1.250000",
            "decimals": 6
          },
          "quote": {
            "status": "available",
            "currency": "USD",
            "unit_price": "1.00",
            "value": "1.250000",
            "price_as_of": "2026-06-18T11:59:59Z"
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

Bulk responses do not expose an aggregate `observed_at`; each account carries
the evidence for its network call. Accounts resolved in the same network group
share a block and observation time. Top-level `errors` is reserved for future
whole-response diagnostics and is currently an empty array on success.

Balance response rules shared by both endpoints:

- `status` is `complete`, `partial`, or `failed`.
- `complete` means every supported balance and quote resolved; unsupported
  asset-network pairs may be skipped without degrading status.
- `partial` means useful balance data exists but at least one supported
  balance or quote failed, or a resolved balance has an unavailable or
  unsupported quote.
- `failed` means supported balance items existed but none resolved.
- A skipped item has `network_slug`, `asset_slug`, and reason
  `asset_not_supported_on_network`.
- `evidence` is an object or `null`. It may expose Bigwig block number, block
  hash, and observation time, but makes no finality claim.
- Public evidence never exposes chain IDs, route IDs, providers, URLs,
  authentication details, capabilities, or other Bigwig internals.
- `balance.raw_amount`, `balance.amount`, quote prices, and quote values are
  exact JSON strings. `balance.decimals` is an integer.
- Quote `status` is `available`, `unavailable`, or `unsupported`. Non-available
  quote fields (`currency`, `unit_price`, `value`, `price_as_of`) are `null`.

Per-account item errors use these stable codes:

| Code | Meaning |
| ---- | ------- |
| `balance_resolution_failed` | Bigwig could not resolve the supported balance. |
| `balance_provider_unavailable` | Balance evidence is temporarily unavailable. |
| `price_resolution_failed` | Price Indexer could not resolve the quote. |
| `price_provider_unavailable` | Quote enrichment is temporarily unavailable. |
| `internal_error` | The item could not be processed safely. |

Bigwig and Price Indexer runtime failures are represented inside a
`200 OK` balance response. They do not become request-wide HTTP errors.

Request-wide errors:

- `400 invalid_request` — malformed/non-JSON body, missing required field,
  wrong field type, or a reserved `chain`/`chain_id`/`chain_slug` alias field.
- `400 invalid_account` — an address is not exactly `0x` plus 40 ASCII hex
  characters.
- `400 unsupported_network` — the network is unknown, non-EVM, legacy, or not
  an exact canonical slug.
- `400 unsupported_asset` — an asset is unknown or not an exact canonical
  global asset slug.
- `400 unsupported_quote_currency` — the normalized quote currency is not
  `USD`, `MXN`, `USDC`, or `BTC`.
- `400 unsupported_as_of` — `as_of.kind` is not `"latest"`.
- `400 empty_accounts` — the bulk account array is empty.
- `400 empty_assets` — the asset array is empty.
- `400 duplicate_account` — a network-scoped account is repeated.
- `400 duplicate_asset` — an exact asset slug is repeated.
- `400 request_too_large` — a public or grouped provider limit is exceeded.
- `503 asset_network_map_unavailable` — the Mother catalog is unconfigured or
  temporarily unavailable.
- `500 internal_error` — Mother API detects inconsistent catalog,
  orchestration, or response-assembly state.

---

### `POST /v1/erc20-transfers/search`

This endpoint is feature-gated by `ERC20_TRANSFERS_ENABLED`. When the gate is
false, which is the default, Mother API does not register the route; callers
receive the normal unmatched-route `404`. The contract below applies when the
gate is explicitly enabled.

Searches a bounded Ethereum mainnet ERC-20 `Transfer` log window for one
watched EVM address. The route accepts catalog `asset_slug` filters, explicit
ERC-20 `contract_addresses`, a mix of both, or no token filter.

Mother API owns public validation, catalog resolution, response shaping, and
error envelopes. Bigwig owns the internal bounded transfer extraction. Mother
API does not index transfers, call EVM RPC directly, infer native transfers,
or enrich prices for this endpoint.

Fields:

| Field | Type | Notes |
| ----- | ---- | ----- |
| `network_slug` | string | Required. Currently only `eth-mainnet`. |
| `address` | string | Required EVM address, normalized to lowercase in execution and response. |
| `direction` | string | Required. One of `any`, `from`, or `to`. |
| `tokens.asset_slugs` | array of strings | Optional exact catalog slugs. Each must resolve to an ERC-20 contract on `network_slug`. |
| `tokens.contract_addresses` | array of strings | Optional ERC-20 contract addresses. Unknown contracts are valid filters. |
| `window.from_block` / `window.to_block` | integers | Block window. |
| `window.from_timestamp` / `window.to_timestamp` | strings | Timestamp window alternative. |
| `window.lookback_seconds` / `window.to` | integer/string | Lookback alternative. `to` is currently `latest`. |

`tokens` may be omitted, `null`, or `{}` for an unfiltered ERC-20 transfer
search.

Token filter semantics:

- `tokens.asset_slugs` are exact canonical Mother API asset slugs. Symbols,
  aliases, and generic names are not accepted.
- Each asset slug must resolve to an ERC-20 contract on `network_slug`.
- Mother API resolves asset slugs first, then appends explicit
  `tokens.contract_addresses`.
- Explicit contract addresses are valid even when unknown to the Mother
  catalog; unknown explicit contracts return `null` catalog metadata.
- All accepted contract addresses normalize to lowercase.
- The merged concrete contract-address set is deduplicated before the public
  token-filter limit is applied.
- If any asset slug is invalid, unknown, unavailable on the requested network,
  native, or non-ERC-20, Mother API rejects the whole request before calling
  Bigwig.

Mother API never silently converts native assets into wrapped assets:

```text
ethereum != wrapped-ether
ETH != WETH
```

If callers want WETH transfer logs, they must request `wrapped-ether` or the
WETH contract address. `Unknown asset slugs never produce empty successful
transfer responses.`

Public limits:

| Limit | Maximum |
| ----- | ------- |
| Unique token filters after resolution and deduplication | 20 |
| Returned rows | 5,000 |

`limits.truncated: true` indicates a valid success response capped by the
upstream extraction row limit.

**Examples:**

Unfiltered search request:

<!-- erc20-transfer-example: request-unfiltered -->

```json
{
  "network_slug": "eth-mainnet",
  "address": "0xabc0000000000000000000000000000000000000",
  "direction": "any",
  "tokens": null,
  "window": {
    "from_block": 18600000,
    "to_block": 18600500
  }
}
```

Unfiltered success response:

<!-- erc20-transfer-example: response-unfiltered -->

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
      "asset_slugs": [],
      "contract_addresses": []
    },
    "resolved_contract_addresses": []
  },
  "transfers": [],
  "limits": {
    "max_rows": 5000,
    "truncated": false
  }
}
```

Asset slug filter request:

<!-- erc20-transfer-example: request-asset-slug -->

```json
{
  "network_slug": "eth-mainnet",
  "address": "0xabc0000000000000000000000000000000000000",
  "direction": "any",
  "tokens": {
    "asset_slugs": ["usdc"],
    "contract_addresses": []
  },
  "window": {
    "from_block": 18600000,
    "to_block": 18600500
  }
}
```

Asset slug filter success response:

<!-- erc20-transfer-example: response-asset-slug -->

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
      "asset_slugs": ["usdc"],
      "contract_addresses": []
    },
    "resolved_contract_addresses": [
      {
        "contract_address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
        "asset_slug": "usdc",
        "symbol": "USDC",
        "decimals": 6,
        "source": "asset_slug"
      }
    ]
  },
  "transfers": [],
  "limits": {
    "max_rows": 5000,
    "truncated": false
  }
}
```

Contract address filter request:

<!-- erc20-transfer-example: request-contract-address -->

```json
{
  "network_slug": "eth-mainnet",
  "address": "0xabc0000000000000000000000000000000000000",
  "direction": "any",
  "tokens": {
    "asset_slugs": [],
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

Contract address filter success response:

<!-- erc20-transfer-example: response-contract-address -->

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
      "asset_slugs": [],
      "contract_addresses": [
        "0x1111111111111111111111111111111111111111"
      ]
    },
    "resolved_contract_addresses": [
      {
        "contract_address": "0x1111111111111111111111111111111111111111",
        "asset_slug": null,
        "symbol": null,
        "decimals": null,
        "source": "contract_address"
      }
    ]
  },
  "transfers": [],
  "limits": {
    "max_rows": 5000,
    "truncated": false
  }
}
```

Mixed filter request:

<!-- erc20-transfer-example: request-mixed-filters -->

```json
{
  "network_slug": "eth-mainnet",
  "address": "0xabc0000000000000000000000000000000000000",
  "direction": "any",
  "tokens": {
    "asset_slugs": ["usdc"],
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

Mixed filter success response:

<!-- erc20-transfer-example: response-mixed-filters -->

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
      "asset_slugs": ["usdc"],
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
      "tx_hash": "0x0000000000000000000000000000000000000000000000000000000000000001",
      "log_index": 12,
      "token": {
        "contract_address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
        "asset_slug": "usdc",
        "symbol": "USDC",
        "decimals": 6
      },
      "from": "0xabc0000000000000000000000000000000000000",
      "to": "0x2222222222222222222222222222222222222222",
      "amount": {
        "raw": "12500000",
        "decimal": "12.5"
      },
      "direction": "from"
    },
    {
      "block_number": 18600002,
      "tx_hash": "0x0000000000000000000000000000000000000000000000000000000000000002",
      "log_index": 13,
      "token": {
        "contract_address": "0x1111111111111111111111111111111111111111",
        "asset_slug": null,
        "symbol": null,
        "decimals": null
      },
      "from": "0x3333333333333333333333333333333333333333",
      "to": "0xabc0000000000000000000000000000000000000",
      "amount": {
        "raw": "1000000",
        "decimal": null
      },
      "direction": "to"
    }
  ],
  "limits": {
    "max_rows": 5000,
    "truncated": false
  }
}
```

Native asset rejection request:

<!-- erc20-transfer-example: request-native-asset-rejection -->

```json
{
  "network_slug": "eth-mainnet",
  "address": "0xabc0000000000000000000000000000000000000",
  "direction": "any",
  "tokens": {
    "asset_slugs": ["ethereum"],
    "contract_addresses": []
  },
  "window": {
    "from_block": 18600000,
    "to_block": 18600500
  }
}
```

Native asset rejection response:

<!-- erc20-transfer-example: error-native-asset-rejection -->

```json
{
  "ok": false,
  "error": {
    "code": "asset_not_erc20_on_network",
    "message": "Asset is not an ERC-20 token on the requested network."
  }
}
```

Unknown slug rejection request:

<!-- erc20-transfer-example: request-unknown-slug-rejection -->

```json
{
  "network_slug": "eth-mainnet",
  "address": "0xabc0000000000000000000000000000000000000",
  "direction": "any",
  "tokens": {
    "asset_slugs": ["missing-but-syntactically-valid"],
    "contract_addresses": []
  },
  "window": {
    "from_block": 18600000,
    "to_block": 18600500
  }
}
```

Unknown slug rejection response:

<!-- erc20-transfer-example: error-unknown-slug-rejection -->

```json
{
  "ok": false,
  "error": {
    "code": "asset_not_found",
    "message": "Asset was not found."
  }
}
```

Too many filters request:

<!-- erc20-transfer-example: request-too-many-filters -->

```json
{
  "network_slug": "eth-mainnet",
  "address": "0xabc0000000000000000000000000000000000000",
  "direction": "any",
  "tokens": {
    "asset_slugs": [],
    "contract_addresses": [
      "0x0000000000000000000000000000000000000001",
      "0x0000000000000000000000000000000000000002",
      "0x0000000000000000000000000000000000000003",
      "0x0000000000000000000000000000000000000004",
      "0x0000000000000000000000000000000000000005",
      "0x0000000000000000000000000000000000000006",
      "0x0000000000000000000000000000000000000007",
      "0x0000000000000000000000000000000000000008",
      "0x0000000000000000000000000000000000000009",
      "0x000000000000000000000000000000000000000a",
      "0x000000000000000000000000000000000000000b",
      "0x000000000000000000000000000000000000000c",
      "0x000000000000000000000000000000000000000d",
      "0x000000000000000000000000000000000000000e",
      "0x000000000000000000000000000000000000000f",
      "0x0000000000000000000000000000000000000010",
      "0x0000000000000000000000000000000000000011",
      "0x0000000000000000000000000000000000000012",
      "0x0000000000000000000000000000000000000013",
      "0x0000000000000000000000000000000000000014",
      "0x0000000000000000000000000000000000000015"
    ]
  },
  "window": {
    "from_block": 18600000,
    "to_block": 18600500
  }
}
```

Too many filters response:

<!-- erc20-transfer-example: error-too-many-filters -->

```json
{
  "ok": false,
  "error": {
    "code": "too_many_token_filters",
    "message": "Too many token filters were requested."
  }
}
```

Truncated success response:

<!-- erc20-transfer-example: response-truncated -->

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
      "asset_slugs": ["usdc"],
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
      "tx_hash": "0x0000000000000000000000000000000000000000000000000000000000000001",
      "log_index": 12,
      "token": {
        "contract_address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
        "asset_slug": "usdc",
        "symbol": "USDC",
        "decimals": 6
      },
      "from": "0xabc0000000000000000000000000000000000000",
      "to": "0x2222222222222222222222222222222222222222",
      "amount": {
        "raw": "12500000",
        "decimal": "12.5"
      },
      "direction": "from"
    },
    {
      "block_number": 18600002,
      "tx_hash": "0x0000000000000000000000000000000000000000000000000000000000000002",
      "log_index": 13,
      "token": {
        "contract_address": "0x1111111111111111111111111111111111111111",
        "asset_slug": null,
        "symbol": null,
        "decimals": null
      },
      "from": "0x3333333333333333333333333333333333333333",
      "to": "0xabc0000000000000000000000000000000000000",
      "amount": {
        "raw": "1000000",
        "decimal": null
      },
      "direction": "to"
    }
  ],
  "limits": {
    "max_rows": 5000,
    "truncated": true
  }
}
```

`token_filters.resolved_contract_addresses` is the exact concrete contract
set searched internally, in search order after normalization and
deduplication. Known contracts are enriched from the Mother catalog with
`asset_slug`, `symbol`, and `decimals`; unknown explicit contracts remain
valid and expose `null` catalog metadata.

`transfers[].amount.raw` is always present as the base-unit integer string
from Bigwig evidence. `transfers[].amount.decimal` is present only when
Mother API knows token decimals from the catalog; the value is a decimal
string with trailing fractional zeros trimmed.

`limits.truncated` mirrors Bigwig's success response. When `true`, the
response is valid but capped by the upstream extraction limit.

Request-wide errors:

- `400 invalid_json` — malformed JSON, non-object body, or missing/non-JSON
  `Content-Type`.
- `400 unknown_field` — request, token, or window object contains an unknown
  field.
- `400 missing_network_slug` — `network_slug` is missing or empty.
- `404 unsupported_network` — `network_slug` is unsupported for transfer
  search.
- `400 invalid_address` — `address` is not an EVM address.
- `400 invalid_direction` — `direction` is not `any`, `from`, or `to`.
- `400 invalid_window` — window shape or bounds are invalid.
- `400 invalid_asset_slug` — an asset slug has invalid syntax.
- `404 asset_not_found` — an asset slug is syntactically valid but unknown.
- `422 asset_not_available_on_network` — an asset exists but is unavailable
  on `network_slug`.
- `422 asset_not_erc20_on_network` — an asset maps to native or non-ERC-20
  support on `network_slug`.
- `503 asset_contract_mapping_unavailable` — catalog resolution or metadata
  enrichment is temporarily unavailable.
- `400 invalid_contract_address` — a contract address is malformed.
- `422 too_many_token_filters` — more than 20 unique token contracts were
  requested after resolution and deduplication.
- `503 extraction_unavailable` — Bigwig extraction is unavailable or returned
  an unavailable dependency class.
- `502 upstream_provider_error` — the upstream RPC provider failed.
- `504 upstream_provider_timeout` — the upstream RPC provider timed out.
- `500 internal_error` — Mother API detects inconsistent catalog or response
  shaping state.

---

## Deprecated endpoints

These endpoints are retained temporarily for compatibility and historical
demos. They are legacy Polymarket-backed prediction routes and are not part of
the current COTO-focused Mother API direction.

Deprecated endpoints receive no new features and may be removed at their
documented removal version. No replacement endpoint is currently promised.
Any future public prediction or intelligence surface must be specified and
accepted separately before becoming part of this contract.

### Deprecated: FIFA World Cup prediction endpoints

| Property       | Value |
| -------------- | ----- |
| Status         | Deprecated |
| Removal target | `v0.2.0` |
| Reason         | Legacy Polymarket/demo surface; not part of the COTO-focused Mother API direction. |
| Replacement    | None currently promised. |

Routes:

- `GET /v1/predictions/fifa-world-cup/winner`
- `GET /v1/predictions/fifa-world-cup/{country}`

Known configured country examples:

- `mexico`
- `argentina`
- `france`
- `colombia`
- `spain`

Successful and error responses from both routes include:

```http
Deprecation: @1781740800
```

The structured date represents June 18, 2026 at 00:00:00 UTC. Mother API does
not emit `Sunset` for these routes because the removal promise is tied to
version `v0.2.0`, not a specific calendar timestamp.

### Deprecated: `GET /v1/predictions/fifa-world-cup/winner`

Returns a live Polymarket-implied, DIS-backed, public/demo-facing snapshot of
the 2026 FIFA World Cup winner prediction market. Mother API calls DIS for
every request and does not call Polymarket directly.

Unknown query parameters are ignored.

Local/dev smoke:

```sh
curl http://localhost:3000/v1/predictions/fifa-world-cup/winner
```

`DIS_BASE_URL` must point at a running DIS instance for a success response. If
DIS is not configured or reachable, the
`prediction_resolver_unavailable` example below is expected.

**Response — `200 OK`:**

```json
{
  "ok": true,
  "event": "2026 FIFA World Cup Winner",
  "event_slug": "fifa-world-cup-2026-winner",
  "odds": [
    {
      "team": "France",
      "probability": "0.18",
      "price": "0.18",
      "currency": "USDC"
    }
  ],
  "source": "polymarket",
  "deterministic": true,
  "captured_at": "2026-06-03T18:20:00Z"
}
```

Fields:

| Field            | Type   | Notes |
| ---------------- | ------ | ----- |
| `ok`             | bool   | Always `true` on success. |
| `event`          | string | Event display name from DIS. |
| `event_slug`     | string | Always `"fifa-world-cup-2026-winner"` for this route. |
| `odds`           | array  | Prediction odds entries. May be empty. |
| `odds[].team`    | string | Team display name. |
| `odds[].probability` | string | Decimal string. Clients must not parse as float. |
| `odds[].price`   | string | Decimal string. Clients must not parse as float. |
| `odds[].currency`| string | Quote currency, currently `"USDC"`. |
| `source`         | string | Prediction source label, currently `"polymarket"`. |
| `deterministic`  | bool   | DIS determinism metadata. |
| `captured_at`    | string | ISO-8601 UTC snapshot timestamp. |

**Errors:**

- `503 prediction_provider_unavailable` - DIS reports the prediction provider
  is unavailable or failed.
- `504 prediction_provider_timeout` - DIS reports the prediction provider
  timed out.
- `503 prediction_resolver_unavailable` - DIS is unconfigured, unreachable,
  or reports an availability failure.
- `504 prediction_resolver_timeout` - Mother API timed out waiting for DIS.
- `502 prediction_resolver_schema_mismatch` - DIS returned HTTP 200, but Mother
  API could not decode the expected success schema.
- `502 prediction_resolver_malformed_response` - DIS returned a non-success
  body that was not a valid error envelope.
- `502 prediction_resolver_error` - DIS returned `internal_error` or an
  unknown code in a valid error envelope.

Examples:

```json
{
  "ok": false,
  "error": {
    "code": "prediction_provider_unavailable",
    "message": "Prediction provider is temporarily unavailable."
  }
}
```

```json
{
  "ok": false,
  "error": {
    "code": "prediction_provider_timeout",
    "message": "Prediction provider timed out."
  }
}
```

```json
{
  "ok": false,
  "error": {
    "code": "prediction_resolver_unavailable",
    "message": "Prediction resolver is temporarily unavailable."
  }
}
```

```json
{
  "ok": false,
  "error": {
    "code": "prediction_resolver_schema_mismatch",
    "message": "Prediction resolver returned an unsupported response."
  }
}
```

```json
{
  "ok": false,
  "error": {
    "code": "prediction_resolver_timeout",
    "message": "Prediction resolver timed out."
  }
}
```

```json
{
  "ok": false,
  "error": {
    "code": "prediction_resolver_malformed_response",
    "message": "Prediction resolver returned a malformed error response."
  }
}
```

```json
{
  "ok": false,
  "error": {
    "code": "prediction_resolver_error",
    "message": "Prediction resolver returned an unclassified error."
  }
}
```

---

### Deprecated: `GET /v1/predictions/fifa-world-cup/{country}`

Returns a live Polymarket-implied, DIS-backed, public/demo-facing prediction
snapshot for one configured World Cup country market. Mother API calls DIS for
every request, trims and lowercases `{country}` before calling DIS, and treats
DIS as the source of truth for supported countries.

Local/dev smoke:

```sh
curl http://localhost:3000/v1/predictions/fifa-world-cup/mexico
```

`DIS_BASE_URL` must point at a running DIS instance for a success response. If
DIS is not configured or reachable, the
`prediction_resolver_unavailable` example below is expected.

**Path parameters:**

| Name      | Type   | Required | Notes |
| --------- | ------ | -------- | ----- |
| `country` | string | Yes      | Case-insensitive country slug, for example `"mexico"`. |

**Response — `200 OK`:**

```json
{
  "ok": true,
  "market": "Mexico to reach Round of 16",
  "country": {
    "slug": "mexico",
    "name": "Mexico"
  },
  "probability": "0.63",
  "price": "0.63",
  "currency": "USDC",
  "source": "polymarket",
  "deterministic": true,
  "captured_at": "2026-06-03T18:20:00Z"
}
```

Fields:

| Field              | Type   | Notes |
| ------------------ | ------ | ----- |
| `ok`               | bool   | Always `true` on success. |
| `market`           | string | Market display name from DIS. |
| `country.slug`     | string | Normalized country slug. |
| `country.name`     | string | Country display name. |
| `probability`      | string | Decimal string. Clients must not parse as float. |
| `price`            | string | Decimal string. Clients must not parse as float. |
| `currency`         | string | Quote currency, currently `"USDC"`. |
| `source`           | string | Prediction source label, currently `"polymarket"`. |
| `deterministic`    | bool   | DIS determinism metadata. |
| `captured_at`      | string | ISO-8601 UTC snapshot timestamp. |

Mother API does not expose DIS-internal fields such as `provider_market`.
DIS owns Polymarket access, provider parsing, probability normalization,
supported-country validation, and provider-specific failure details.

**Errors:**

- `400 unsupported_prediction_subject` - The country is empty or not supported
  for this event.
- `503 prediction_provider_unavailable` - DIS reports the prediction provider
  is unavailable or failed.
- `504 prediction_provider_timeout` - DIS reports the prediction provider
  timed out.
- `503 prediction_resolver_unavailable` - DIS is unconfigured, unreachable,
  or reports an availability failure.
- `504 prediction_resolver_timeout` - Mother API timed out waiting for DIS.
- `502 prediction_resolver_schema_mismatch` - DIS returned HTTP 200, but Mother
  API could not decode the expected success schema.
- `502 prediction_resolver_malformed_response` - DIS returned a non-success
  body that was not a valid error envelope.
- `502 prediction_resolver_error` - DIS returned `internal_error` or an
  unknown code in a valid error envelope.

Examples:

```json
{
  "ok": false,
  "error": {
    "code": "unsupported_prediction_subject",
    "message": "Prediction subject is not supported for this event."
  }
}
```

```json
{
  "ok": false,
  "error": {
    "code": "prediction_provider_unavailable",
    "message": "Prediction provider is temporarily unavailable."
  }
}
```

```json
{
  "ok": false,
  "error": {
    "code": "prediction_provider_timeout",
    "message": "Prediction provider timed out."
  }
}
```

```json
{
  "ok": false,
  "error": {
    "code": "prediction_resolver_unavailable",
    "message": "Prediction resolver is temporarily unavailable."
  }
}
```

```json
{
  "ok": false,
  "error": {
    "code": "prediction_resolver_schema_mismatch",
    "message": "Prediction resolver returned an unsupported response."
  }
}
```

```json
{
  "ok": false,
  "error": {
    "code": "prediction_resolver_timeout",
    "message": "Prediction resolver timed out."
  }
}
```

```json
{
  "ok": false,
  "error": {
    "code": "prediction_resolver_malformed_response",
    "message": "Prediction resolver returned a malformed error response."
  }
}
```

```json
{
  "ok": false,
  "error": {
    "code": "prediction_resolver_error",
    "message": "Prediction resolver returned an unclassified error."
  }
}
```

---

## Stable endpoints (continued)

### `GET /v1/search-engine`

Resolves broad Sentinel search queries against Mother API-owned global
assets. Unknown queries return a successful unresolved response with
recommendations instead of a 404, so the frontend never gets a blind
dead-end.

**Query parameters:**

| Name | Type   | Required | Notes                                                  |
| ---- | ------ | -------- | ------------------------------------------------------ |
| `q`  | string | Yes      | Free-text query. Trimmed. Must be 128 characters or fewer after trimming. |

**Response — `200 OK` (resolved):**

```json
{
  "ok": true,
  "type": "resolve",
  "resolved": true,
  "query": {
    "raw": "usdc coin usd",
    "normalized": "usdc coin usd"
  },
  "result": {
    "kind": "asset",
    "canonical_path": "/assets/usdc",
    "resource_url": "/v1/assets/usdc",
    "confidence": "alias_exact",
    "asset": {
      "asset_id": "usdc",
      "symbol": "USDC",
      "name": "USD Coin",
      "category": "crypto"
    }
  }
}
```

**Response — `200 OK` (unresolved):**

```json
{
  "ok": true,
  "type": "resolve",
  "resolved": false,
  "query": {
    "raw": "some unknown thing",
    "normalized": "some unknown thing"
  },
  "result": {
    "kind": "unknown",
    "message": "Iron Burrow does not know this query publicly yet. Showing related recommendations instead.",
    "recommendations": [
      {
        "kind": "asset",
        "canonical_path": "/assets/bitcoin",
        "asset": {
          "asset_id": "bitcoin",
          "symbol": "BTC",
          "name": "Bitcoin",
          "category": "crypto"
        },
        "reason": "related_public_asset"
      }
    ]
  }
}
```

Top-level fields:

| Field             | Type   | Notes                                                       |
| ----------------- | ------ | ----------------------------------------------------------- |
| `ok`              | bool   | Always `true` on success (resolved or not).                 |
| `type`            | string | Always `"resolve"`.                                         |
| `resolved`        | bool   | `true` when a confident asset match was found.              |
| `query.raw`       | string | Trimmed query as received.                                  |
| `query.normalized`| string | Normalized form used for matching.                          |
| `result.kind`     | string | `"asset"` when `resolved=true`, `"unknown"` otherwise.      |

Resolved `result` (when `result.kind == "asset"`):

| Field             | Type   | Notes                                                       |
| ----------------- | ------ | ----------------------------------------------------------- |
| `canonical_path`  | string | Canonical public path of the asset.                         |
| `resource_url`    | string | Mother API resource URL (`/v1/assets/{slug}`).              |
| `confidence`      | string | Match confidence label (e.g., `"alias_exact"`).             |
| `asset`           | object | Asset summary (`asset_id`, `symbol`, `name`, `category`).   |

Unresolved `result` (when `result.kind == "unknown"`):

| Field             | Type   | Notes                                                       |
| ----------------- | ------ | ----------------------------------------------------------- |
| `message`         | string | Stable human-readable explanation.                          |
| `recommendations` | array  | Zero or more related public asset suggestions.              |

Recommendation entry:

| Field            | Type   | Notes                                              |
| ---------------- | ------ | -------------------------------------------------- |
| `kind`           | string | Always `"asset"` in this contract revision.        |
| `canonical_path` | string | Canonical public path of the recommended asset.    |
| `asset`          | object | Asset summary.                                     |
| `reason`         | string | Reason code (e.g., `"related_public_asset"`).      |

**Errors:**

- `400 missing_query` — `q` is missing or empty after trimming.
- `400 query_too_long` — Trimmed `q` exceeds 128 characters.
- `503 database_unavailable` — `DATABASE_URL` is unset or Postgres is
  unreachable.

---

## Price block

Asset list and asset detail responses include a `price` block. The block
is **always present**, even when price enrichment is disabled or upstream
lookups fail. Unavailable prices return a stable shape rather than a
missing field.

```json
{
  "status": "available",
  "price": "1.0001",
  "quote_currency": "USD",
  "source_type": "coingecko",
  "confidence_label": "high",
  "is_fallback": false,
  "is_derived": false,
  "recorded_at": "2026-05-26T12:00:05Z",
  "warning": null
}
```

Fields:

| Field              | Type            | Notes                                                                                                     |
| ------------------ | --------------- | --------------------------------------------------------------------------------------------------------- |
| `status`           | string          | `"available"` or `"unavailable"`.                                                                         |
| `price`            | string \| null  | Decimal string. Clients must not parse as float. `null` when `status == "unavailable"`.                   |
| `quote_currency`   | string \| null  | Quote currency code (e.g., `"USD"`). `null` when unavailable.                                             |
| `source_type`      | string \| null  | Upstream source label (e.g., `"chainlink"`, `"coingecko"`). `null` when unavailable.                      |
| `confidence_label` | string \| null  | Optional confidence label from the price source.                                                          |
| `is_fallback`      | bool            | `true` when the price came from a fallback source.                                                        |
| `is_derived`       | bool            | `true` when the price was derived rather than directly observed.                                          |
| `recorded_at`      | string \| null  | ISO-8601 UTC timestamp of the price observation. `null` when unavailable.                                 |
| `warning`          | string \| null  | Optional human-readable warning string.                                                                   |

Unavailable shape (stable):

```json
{
  "status": "unavailable",
  "price": null,
  "quote_currency": null,
  "source_type": null,
  "confidence_label": null,
  "is_fallback": false,
  "is_derived": false,
  "recorded_at": null,
  "warning": null
}
```

Price enrichment is enabled only when both `PRICE_INDEXER_URL` and
`PRICE_QL_INTERNAL_TOKEN` are configured. Missing or failing price
configuration does **not** fail the asset responses; the unavailable
shape above is returned instead.

The price-indexer Query Layer owns price availability, derivation, and
historical price data. Mother API consumes it read-only and does not
promise any field beyond those listed here.

---

## Error envelope

All `4xx` and `5xx` responses from Mother API endpoints under this
contract use the same JSON envelope:

```json
{
  "ok": false,
  "error": {
    "code": "missing_query",
    "message": "Query parameter `q` is required."
  }
}
```

Fields:

| Field           | Type   | Notes                                                  |
| --------------- | ------ | ------------------------------------------------------ |
| `ok`            | bool   | Always `false`.                                        |
| `error.code`    | string | Stable machine-readable code. Clients should branch on this. |
| `error.message` | string | Human-readable explanation. May change wording; not a contract. |

### Error code catalogue

| HTTP | `error.code`            | Trigger                                                                |
| ---- | ----------------------- | ---------------------------------------------------------------------- |
| 400  | `invalid_request`       | A JSON body is malformed/missing required fields, includes a reserved balance network alias field, or non-balance public parameters are invalid or incompatible. |
| 400  | `invalid_account`       | A balance account address is not `0x` plus 40 ASCII hexadecimal characters. |
| 400  | `unsupported_network`   | A balance request uses an unknown, non-EVM, legacy, or non-canonical network slug. |
| 400  | `unsupported_asset`     | A balance request uses an unknown or non-canonical global asset slug. |
| 400  | `unsupported_quote_currency` | A balance request uses a quote currency outside `USD`, `MXN`, `USDC`, and `BTC`. |
| 400  | `unsupported_as_of`     | A balance request asks for anything other than the latest snapshot. |
| 400  | `empty_accounts`        | A bulk balance request contains no accounts. |
| 400  | `empty_assets`          | A balance request contains no assets. |
| 400  | `duplicate_account`     | A bulk balance request repeats a network-scoped account. |
| 400  | `duplicate_asset`       | A balance request repeats an exact asset slug. |
| 400  | `request_too_large`     | A balance request exceeds a public or grouped provider limit. |
| 400  | `invalid_limit`         | `limit` query parameter is not a positive integer.                     |
| 400  | `invalid_json`          | A JSON body is malformed, not an object, or sent without JSON content type. |
| 400  | `unknown_field`         | A strict JSON request object contains an unsupported field.             |
| 400  | `missing_network_slug`  | A transfer search request omits `network_slug` or sends it empty.       |
| 400  | `invalid_address`       | A transfer search address is not an EVM address.                       |
| 400  | `invalid_direction`     | A transfer search direction is not `any`, `from`, or `to`.             |
| 400  | `invalid_window`        | A transfer search window is missing, malformed, or reversed.           |
| 400  | `invalid_asset_slug`    | A transfer token `asset_slug` filter has invalid syntax.               |
| 400  | `invalid_contract_address` | A transfer token `contract_addresses` filter is malformed.          |
| 400  | `missing_query`         | `q` query parameter is missing or empty after trimming.                |
| 400  | `query_too_long`        | Trimmed `q` exceeds 128 characters.                                    |
| 404  | `asset_not_found`       | Asset detail lookup failed, price-indexer has no requested signal, or a transfer asset slug is unknown. |
| 404  | `unsupported_network`   | A transfer search request uses a network unsupported by that endpoint. |
| 422  | `asset_not_available_on_network` | A transfer asset filter exists but is unavailable on the requested network. |
| 422  | `asset_not_erc20_on_network` | A transfer asset filter is native or not ERC-20 on the requested network. |
| 422  | `too_many_token_filters` | A transfer search exceeds its unique token-filter limit.              |
| 502  | `upstream_auth_failed`  | Mother API could not authenticate to price-indexer.                    |
| 502  | `price_indexer_error`   | Price-indexer failed while handling a valid signal request.            |
| 502  | `upstream_invalid_response` | Price-indexer returned malformed or unexpected JSON.               |
| 502  | `upstream_provider_error` | Bigwig's upstream RPC provider failed during transfer extraction.    |
| 503  | `database_unavailable`  | `DATABASE_URL` is unset or Postgres is unreachable.                    |
| 503  | `asset_network_map_unavailable` | The balance catalog is unconfigured or temporarily unavailable. |
| 503  | `asset_contract_mapping_unavailable` | Transfer asset contract mapping is unconfigured or temporarily unavailable. |
| 503  | `extraction_unavailable` | Bigwig ERC-20 transfer extraction is disabled, unconfigured, unreachable, or malformed after the Mother route gate is enabled. |
| 503  | `price_indexer_unavailable` | Price-indexer is unconfigured, unreachable, or timed out.          |
| 504  | `upstream_provider_timeout` | Bigwig's upstream RPC provider timed out during transfer extraction. |
| 400  | `unsupported_prediction_subject` | Requested prediction country is unsupported for the event.    |
| 503  | `prediction_provider_unavailable` | DIS reports the prediction provider is unavailable or failed. |
| 504  | `prediction_provider_timeout` | DIS reports the prediction provider timed out.                    |
| 503  | `prediction_resolver_unavailable` | DIS is unconfigured, unreachable, or reports an availability failure. |
| 504  | `prediction_resolver_timeout` | Mother API timed out waiting for DIS. |
| 502  | `prediction_resolver_schema_mismatch` | DIS returned HTTP 200, but Mother API could not decode the expected success schema. |
| 502  | `prediction_resolver_malformed_response` | DIS returned a non-success body that was not a valid error envelope. |
| 502  | `prediction_resolver_error` | DIS returned `internal_error` or an unknown code in a valid error envelope. |
| 500  | `internal_error`        | Mother API encountered an unexpected or internally inconsistent state. |

`error.code` values listed above are stable. New codes may be added in
future contract revisions. Clients must tolerate unknown codes by
falling back to the HTTP status.

---

## Out of scope (not part of this contract)

The following are explicitly **not** promised by this contract and must
not be assumed to exist or behave consistently if encountered:

- Public price routes outside the asset signal surface (e.g.,
  `/v1/prices/*`).
- Event, holder, or network indexing endpoints.
- Admin, explorer, account, or tracked-token routes.
- API keys, bearer auth, billing, x402, or rate limiting on inbound
  requests.
- In-process response caching headers (e.g., custom `X-Cache-*`).
- Read-model asset sync feeds. A sync surface for
  `iron-burrow-read-model` requires an accepted proposal, implementation,
  and CONTRACTS.md revision before it becomes part of this surface.
- Aave V3 realized yield or any other DeFi-protocol-specific endpoint.
  Mother API consumes
  [`iron-burrow-defi-intelligence-service`](docs/specs/SPEC-001-dis-aave-v3-realized-yield.md)
  internally for protocol intelligence; a public wrapper requires a
  separate accepted spec and a CONTRACTS.md revision before it becomes
  part of this surface.
- Direct exposure of internal DIS or read-model service shapes. Price
  signal endpoints preserve price-indexer signal payload fields inside
  Mother API envelopes; the deprecated prediction endpoints expose only the
  sanitized SPEC-004 public shapes until removal; other public responses are
  owned by this contract, not by upstream services.

Adding any of the above to the public surface requires an accepted RFC
or spec under [docs/](docs/) and a coordinated update to this file.
