---
status: accepted
owner: iron-burrow
last_reviewed: 2026-07-30
agent_edit_policy: update_when_relevant
external_contract: iron-burrow-price-indexer/CONTRACTS.md@2026-06-02
---

# SPEC-002 - Asset Detail Enrichment for Data Lab Asset Pages

Accepted implementation specification for Mother API asset-detail enrichment
used by the future authenticated Data Lab asset page, backed by the
`iron-burrow-price-indexer` Query Layer.

This spec was split out of the original `SPEC-002` draft, which mixed two
distinct consumers. The low-level, agent-facing signal endpoints
(`/v1/assets/{slug}/signal/*`) now live in `SPEC-003 - Price Signal Endpoints
for MCP and Agents`. This spec covers only the asset-page enrichment use case.

This spec records how the implemented asset-detail endpoint composes optional
price intelligence. It does not authorize Mother API to re-own, recalculate,
or reinterpret price-derived intelligence. `price-indexer` owns price
observations, bucketization, statistics, trend formulas, confidence, and
warning semantics. The binding public behavior is in
[`CONTRACTS.md`](../../CONTRACTS.md).

## Status

Accepted and implemented as the current Alpha asset-detail composition. The
forward product delivery target is an authenticated Data Lab page under
`/app`, not an expansion of `/v1`. A later focused implementation spec must
use the shared application service directly, define browser authorization and
presentation behavior, and make a separate compatibility decision for the
existing Alpha route. The dedicated strict signal endpoints remain separately
defined by `SPEC-003` while they continue to exist.

## Purpose

The current Alpha transport for the asset-detail composition is:

```http
GET /v1/assets/{slug}
```

The same composition supports a future Data Lab asset page where one
application-service call returns everything needed to render one asset:

- asset identity
- asset network maps / asset metadata
- latest price block
- optional price stats
- optional price trend
- optional price series snippet for Data Lab charting

All three optional signals, including `priceSeries`, are part of the accepted
asset-detail composition. The Data Lab asset page is the intended charting
surface, so `priceSeries` ships alongside `priceStats` and `priceTrend`.

The goal is one application-service composition for the Data Lab asset page.
The goal is **not** to turn the page into a new public JSON endpoint. The
existing strict signal routes remain the separately implemented behavior
recorded by `SPEC-003`; no new `/v1` signal endpoint is authorized by this
spec.

The upstream source documents for this spec are:

- `iron-burrow-price-indexer/CONTRACTS.md@2026-06-02`.
- `iron-burrow-price-indexer/docs/rfcs/RFC-003-deterministic-price-stats-and-trend-signals.md`.
- `iron-burrow-price-indexer/docs/adr/ADR-001-unify-price-series-on-window-granularity.md`.

## Existing behavior

The `/v1/assets/{slug}` base response shape must remain stable. It returns:

- `ok`, `type: "asset"`
- `asset` (identity summary)
- `price` (latest price block, always present)
- `asset_network_maps`

The endpoint attempts latest price enrichment as part of base asset detail.
The latest price block is always attempted in the requested `quoteCurrency`;
when the price-indexer client is disabled or the lookup fails, the endpoint
returns a price block with `status: "unavailable"`. Mother API passes through
price-indexer-owned direct or derived price metadata and does not calculate
currency conversions.

## Enrichment model

Optional enrichments are requested through an `include` query parameter.

```http
GET /v1/assets/{slug}?include=priceStats,priceTrend,priceSeries&quoteCurrency=USD&window=24h&granularity=1h
```

### Query parameters

| Name            | Required | Default          | Allowed values                                  | Notes |
| --------------- | -------- | ---------------- | ----------------------------------------------- | ----- |
| `include`       | No       | none             | comma-separated: `priceStats`, `priceTrend`, `priceSeries` | Unknown tokens are ignored. |
| `quoteCurrency` | No       | `USD`            | `USD`, `MXN`, `USDC`, `BTC`                      | Applied to the latest price and requested enrichments. |
| `window`        | No       | `24h`            | `1h`, `24h`, `7d`, `30d`                         | Applied to requested enrichments. |
| `granularity`   | No       | upstream default | `5m`, `1h`, `1d`                                 | Forwarded only when provided. |

Recommended `include` values:

- `priceStats`
- `priceTrend`
- `priceSeries`

Rules:

- When `include` is absent, the endpoint returns the existing stable shape plus
  the latest price block in the requested `quoteCurrency`, with no signal
  enrichment.
- `include` tokens are matched case-insensitively after trimming. Unknown
  tokens are ignored rather than rejected, consistent with the Mother API
  convention that unknown query parameters are ignored.
- `quoteCurrency` always applies to the latest price and also applies uniformly
  to requested enrichments. `window` and `granularity` only take effect when at
  least one enrichment is requested. Signal parameters follow the same allowed
  values and forwarding rules as `SPEC-003` and must obey ADR-001.

### Enrichment doctrine

- **Latest price** enrichment is always attempted, because it is part of
  existing asset-detail behavior.
- **Optional enrichments** (`priceStats`, `priceTrend`, `priceSeries`) are
  attempted only when requested via `include`.
- **Failure of price or optional enrichments must not fail the whole asset
  page** when the base asset exists.
- The response must be **explicit and honest** about enrichment failures.

## Price-indexer delegation

Mother API must not calculate stats, trend, or price series locally. It
delegates to `price-indexer`. The provider contract is the source of truth:

- `iron-burrow-price-indexer/CONTRACTS.md@2026-06-02`
- `iron-burrow-price-indexer/docs/rfcs/RFC-003-deterministic-price-stats-and-trend-signals.md`
- `iron-burrow-price-indexer/docs/adr/ADR-001-unify-price-series-on-window-granularity.md`

Per ADR-001, the current time-series doctrine is `window`/`granularity`, not
legacy `range`/`resolution`. Mother API must never send `range`, `resolution`,
`from`, `to`, `interval`, `sourceType`, `limit`, or `beforeId`.

Upstream mapping for each requested enrichment:

| Include token | Upstream endpoint     | Notes |
| ------------- | --------------------- | ----- |
| `priceStats`  | `GET /prices/stats`   | Pass-through of upstream stats fields and `warnings`. |
| `priceTrend`  | `GET /prices/trend`   | Pass-through of upstream trend fields and `warnings`. |
| `priceSeries` | `GET /prices/series`  | Data Lab charting series; pass-through of upstream `points` and `meta`. Uses the same `window`/`granularity` model and obeys ADR-001. |

Shared upstream parameters (echoing `SPEC-003`):

| Name            | Required upstream | Mother default | Allowed values              | Notes |
| --------------- | ----------------- | -------------- | --------------------------- | ----- |
| `slug`          | Yes               | path `slug`    | canonical asset slug        | Trimmed and lowercased upstream. |
| `quoteCurrency` | No                | `USD`          | `USD`, `MXN`, `USDC`, `BTC` | |
| `window`        | Yes               | `24h`          | `1h`, `24h`, `7d`, `30d`    | Mother defaults before forwarding. |
| `granularity`   | No                | per window     | `5m`, `1h`, `1d`            | Forward only when provided. |

Allowed `window`/`granularity` combinations are identical to `SPEC-003` and the
upstream `/prices/series` matrix:

| `window` | Default `granularity` | Allowed granularities |
| -------- | --------------------- | --------------------- |
| `1h`     | `5m`                  | `5m`                  |
| `24h`    | `1h`                  | `5m`, `1h`            |
| `7d`     | `1h`                  | `1h`                  |
| `30d`    | `1d`                  | `1d`                  |

`asOf` is not exposed by Mother API V0.

## Response shape

When `include` is present, the response extends the existing asset-detail shape
with a `signals` object and an `enrichment_errors` array. Field names follow
the existing Mother API snake_case envelope style (`asset_network_maps`,
`asset_id`, `quote_currency`).

```json
{
  "ok": true,
  "type": "asset",
  "asset": {
    "asset_id": "ethereum",
    "symbol": "ETH",
    "name": "Ethereum",
    "category": "crypto",
    "canonical_path": "/assets/ethereum"
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
  "asset_network_maps": [],
  "signals": {
    "price_stats": null,
    "price_trend": null
  },
  "enrichment_errors": [
    {
      "source": "price_stats",
      "code": "price_indexer_unavailable",
      "message": "Price stats are temporarily unavailable."
    }
  ]
}
```

Rules:

- `signals` is present only when `include` requested at least one known
  enrichment.
- Each requested enrichment appears as a key under `signals`
  (`price_stats`, `price_trend`, `price_series`). On success the value is the
  pass-through upstream payload; on failure the value is `null` and a
  corresponding entry is added to `enrichment_errors`.
- `enrichment_errors` is present only when `include` requested at least one
  known enrichment. It is an empty array when all requested enrichments
  succeed.
- Successful upstream `warnings` arrays are preserved exactly inside each
  signal payload. Warnings are not failures and do not produce an
  `enrichment_errors` entry.
- Decimal values remain JSON strings. Mother API must not parse decimal price,
  ratio, slope, or volatility fields into floating-point values.
- Mother API must not reinterpret upstream confidence, trend direction, or
  warning codes.

Each `enrichment_errors` entry:

| Field     | Type   | Notes |
| --------- | ------ | ----- |
| `source`  | string | The enrichment that failed: `price_stats`, `price_trend`, `price_series`. |
| `code`    | string | A stable Mother API enrichment error code (see below). |
| `message` | string | A Mother API-owned, non-specific public message. |

## Partial failure behavior

The governing rule:

```
Base asset failure   -> endpoint failure.
Enrichment failure   -> partial response with explicit enrichment error.
```

- If the base asset does **not** exist, the endpoint fails exactly as today
  (`404 asset_not_found`).
- If the base asset exists, the endpoint returns `200 OK` even when latest
  price or any requested enrichment fails.
- Latest price failure is already represented by the existing price block
  `status: "unavailable"`; that behavior is unchanged.
- Each failed optional enrichment yields a `null` signal value plus one
  explicit `enrichment_errors` entry. Failures are never hidden.

### Enrichment error codes

Mother API owns these enrichment error codes. They mirror the upstream-failure
classes already mapped in `SPEC-003`, but are surfaced per-enrichment instead
of as a top-level error envelope:

| Enrichment failure cause          | `enrichment_errors[].code`   |
| --------------------------------- | ---------------------------- |
| Upstream `400 INVALID_REQUEST`    | `invalid_request`            |
| Upstream `404 NOT_FOUND`          | `signal_not_available`       |
| Upstream `401 UNAUTHORIZED`       | `upstream_auth_failed`       |
| Upstream `500 INTERNAL_ERROR`     | `price_indexer_error`        |
| Timeout or connection failure     | `price_indexer_unavailable`  |
| Malformed upstream response       | `upstream_invalid_response`  |
| Price-indexer client disabled     | `price_indexer_unavailable`  |

Mother API must not propagate upstream `error.message` verbatim into
`enrichment_errors[].message`. Public messages are owned by Mother API.

`CONTRACTS.md` records the implemented public fields and enrichment error
codes. Any future public behavior change must update it in the same change.

## Configuration

This spec reuses the existing price-indexer client and configuration. No new
configuration variables are introduced.

| Variable                   | Default | Description |
| -------------------------- | ------- | ----------- |
| `PRICE_INDEXER_URL`        | unset   | Internal price-indexer base URL, for example `http://price-indexer:3010`. |
| `PRICE_QL_INTERNAL_TOKEN`  | unset   | Bearer token sent to private price-indexer routes. |
| `PRICE_INDEXER_TIMEOUT_MS` | `2000`  | Per-request timeout in milliseconds. |

Behavior:

- If `PRICE_INDEXER_URL` or `PRICE_QL_INTERNAL_TOKEN` is unset, the
  price-indexer client remains disabled.
- When the client is disabled and an enrichment is requested, the enrichment
  yields a `null` signal value and a `price_indexer_unavailable`
  `enrichment_errors` entry. The base asset response still returns `200 OK`.
- Invalid `PRICE_INDEXER_TIMEOUT_MS` remains a startup configuration error.

## Implemented design

- The shared [price-indexer adapter](../../src/adapters/price_indexer/) owns
  the client, token convention, timeout setting, and base URL configuration.
  Mother API does not create a second client.
- Stats, trend, and series helpers use the same `window`/`granularity` request
  model. The asset service composes them; the direct agent signal routes remain
  the separate consumer described by `SPEC-003`.
- The handler and composition service live in the existing assets routing
  family: [routes](../../src/adapters/http/routes/assets.rs) and
  [application service](../../src/application/assets/service.rs).
- Successful upstream payloads are passed through as raw JSON. Typed upstream
  response parsing must avoid
  `deny_unknown_fields`, because `price-indexer` may add informational fields
  without a contract break.
- Enrichment lookups are independent: one failing enrichment does not
  prevent the others or the base asset response.

## Non-goals

This spec explicitly does not cover:

- Implementing code in this documentation change.
- Updating `CONTRACTS.md` before the endpoint behavior ships.
- The dedicated `/v1/assets/{slug}/signal/*` endpoints (owned by `SPEC-003`).
- MCP-specific or strict single-signal endpoints.
- A dedicated stablecoin depeg or stability endpoint.
- Billing, rate-limit, API key, or x402 redesign.
- Read-model caching or materialized views.
- Recalculating stats, trend, or series in Mother API.
- Changing price-indexer formulas or contracts.
- Exposing upstream `asOf` in V0.
- Extending the price-indexer `window` and `granularity` matrix.

## Recorded decisions

- `priceSeries` passes through the upstream `points` and `meta` payload for
  Data Lab charting. A bounded presentation shape is a future UI refinement,
  not a reason to change the data contract.
- Requested enrichments use uniform `quoteCurrency`, `window`, and
  `granularity` parameters. Per-enrichment overrides are not part of this
  contract.

## Completion evidence

The implemented route and regression tests prove:

- Existing `/v1/assets/{slug}` response shape is unchanged when `include` is
  absent.
- Latest price enrichment is always attempted with the normalized
  `quoteCurrency`, defaulting to `USD`.
- `include=priceStats` calls `/prices/stats` with the exact mapped parameters.
- `include=priceTrend` calls `/prices/trend` with the exact mapped parameters.
- `include=priceSeries` calls `/prices/series` with the exact mapped parameters
  and passes upstream `points` and `meta` through.
- Omitted `quoteCurrency` becomes `USD`; omitted `window` becomes `24h`;
  omitted `granularity` is not sent upstream.
- `range` and `resolution` are never sent upstream.
- Upstream warnings are preserved inside signal payloads on success.
- A failing enrichment produces a `null` signal value and one explicit
  `enrichment_errors` entry, while the endpoint still returns `200 OK` when the
  base asset exists. This holds for `priceSeries` as well as `priceStats` and
  `priceTrend`.
- A non-existent base asset still returns `404 asset_not_found`.
- Unknown `include` tokens are ignored.
- Mother API does not calculate stats, trend, or series locally.
