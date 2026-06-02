---
status: contract
owner: iron-burrow
last_reviewed: 2026-06-02
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

## Endpoints

| Method | Path                                    | Auth | Notes                                                       |
| ------ | --------------------------------------- | ---- | ----------------------------------------------------------- |
| `GET`  | `/health`                               | None | Dependency-free liveness probe.                             |
| `GET`  | `/v1/status`                            | None | Informational readiness with dependency checks.             |
| `GET`  | `/v1/assets`                            | None | Lists active global assets with optional price enrichment.  |
| `GET`  | `/v1/assets/{slug}`                     | None | Returns one active asset, its chain maps, and a price block. |
| `GET`  | `/v1/assets/{slug}/signal/price-stats`  | None | Returns a strict price statistics signal for one asset.      |
| `GET`  | `/v1/assets/{slug}/signal/price-trend`  | None | Returns a strict price trend signal for one asset.           |
| `GET`  | `/v1/resolve`                           | None | Resolves a Sentinel search query against global assets.     |

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
    "price_indexer": "not_connected",
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
| `checks.price_indexer` | string | Currently always `"not_connected"`. May expand to richer states in the future. |
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

Returns one active asset, the network-specific chain maps the UI can use
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
| `quoteCurrency` | string | No       | `USD`   | `USD`, `MXN`, `USDC`, `BTC` | Applies only when at least one known enrichment is requested. |
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
  "chain_maps": [
    {
      "network": {
        "slug": "eth-mainnet",
        "name": "Ethereum Mainnet",
        "caip2": "eip155:1"
      },
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
  "chain_maps": [
    {
      "network": {
        "slug": "eth-mainnet",
        "name": "Ethereum Mainnet",
        "caip2": "eip155:1"
      },
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
  "chain_maps": [
    {
      "network": {
        "slug": "eth-mainnet",
        "name": "Ethereum Mainnet",
        "caip2": "eip155:1"
      },
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
| `chain_maps` | array  | Chain map entries (see below). May be empty.                |
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

Chain map entry:

| Field             | Type            | Notes                                                                  |
| ----------------- | --------------- | ---------------------------------------------------------------------- |
| `network.slug`    | string          | Network slug (e.g., `"eth-mainnet"`).                                  |
| `network.name`    | string          | Network display name.                                                  |
| `network.caip2`   | string \| null  | CAIP-2 identifier when known.                                          |
| `is_native`       | bool            | `true` when the asset is the network's native asset.                   |
| `address`         | string \| null  | Token contract address. `null` for native assets or when not applicable. |

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

### `GET /v1/resolve`

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
| 400  | `invalid_request`       | Price signal query parameters are unsupported or incompatible.         |
| 400  | `invalid_limit`         | `limit` query parameter is not a positive integer.                     |
| 400  | `missing_query`         | `q` query parameter is missing or empty after trimming.                |
| 400  | `query_too_long`        | Trimmed `q` exceeds 128 characters.                                    |
| 404  | `asset_not_found`       | Asset detail lookup failed, or price-indexer has no requested signal.  |
| 502  | `upstream_auth_failed`  | Mother API could not authenticate to price-indexer.                    |
| 502  | `price_indexer_error`   | Price-indexer failed while handling a valid signal request.            |
| 502  | `upstream_invalid_response` | Price-indexer returned malformed or unexpected JSON.               |
| 503  | `database_unavailable`  | `DATABASE_URL` is unset or Postgres is unreachable.                    |
| 503  | `price_indexer_unavailable` | Price-indexer is unconfigured, unreachable, or timed out.          |

`error.code` values listed above are stable. New codes may be added in
future contract revisions. Clients must tolerate unknown codes by
falling back to the HTTP status.

---

## Out of scope (not part of this contract)

The following are explicitly **not** promised by this contract and must
not be assumed to exist or behave consistently if encountered:

- Public price routes outside the asset signal surface (e.g.,
  `/v1/prices/*`).
- Event, holder, or chain indexing endpoints.
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
  Mother API envelopes; other public responses are owned by this contract,
  not by upstream services.

Adding any of the above to the public surface requires an accepted RFC
or spec under [docs/](docs/) and a coordinated update to this file.
