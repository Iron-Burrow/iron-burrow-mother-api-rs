---
status: draft
owner: iron-burrow
last_reviewed: 2026-06-02
agent_edit_policy: update_when_relevant
external_contract: iron-burrow-price-indexer/CONTRACTS.md@2026-06-02
---

# SPEC-002 - Asset Detail Enrichment for UI and Demo

Mother API asset-detail enrichment for UI surfaces and the demo, backed by the
`iron-burrow-price-indexer` Query Layer.

This spec was split out of the original `SPEC-002` draft, which mixed two
distinct consumers. The low-level, agent-facing signal endpoints
(`/v1/assets/{slug}/signal/*`) now live in `SPEC-003 - Price Signal Endpoints
for MCP and Agents`. This spec covers only the asset-page enrichment use case.

This spec defines how the existing asset-detail endpoint composes optional
price intelligence. It does not authorize Mother API to re-own, recalculate,
or reinterpret price-derived intelligence. `price-indexer` owns price
observations, bucketization, statistics, trend formulas, confidence, and
warning semantics.

## Purpose

The asset-detail endpoint is the asset-page endpoint:

```http
GET /v1/assets/{slug}
```

It should support a UI/demo use case where a single call can return everything
needed to render an asset page:

- asset identity
- chain maps / asset metadata
- latest price block
- optional price stats
- optional price trend
- optional price series snippet for UI/demo charting

All three optional signals, including `priceSeries`, are in scope for V0. The
asset page is the UI/demo charting surface, so `priceSeries` ships in the same
V0 asset-detail enrichment work as `priceStats` and `priceTrend`.

The goal is one round trip for the asset page. The goal is **not** to make the
asset page a strict signal endpoint. Strict, single-signal access for agents
is `SPEC-003`.

The upstream source documents for this spec are:

- `iron-burrow-price-indexer/CONTRACTS.md@2026-06-02`.
- `iron-burrow-price-indexer/docs/rfcs/RFC-003-deterministic-price-stats-and-trend-signals.md`.
- `iron-burrow-price-indexer/docs/adr/ADR-001-unify-price-series-on-window-granularity.md`.

## Existing behavior

The current `/v1/assets/{slug}` behavior must remain stable. It returns:

- `ok`, `type: "asset"`
- `asset` (identity summary)
- `price` (latest price block, always present)
- `chain_maps`

Today the endpoint already attempts latest price enrichment as part of base
asset detail. That behavior is preserved. The latest price block is always
attempted; when the price-indexer client is disabled or the lookup fails, the
endpoint already returns a price block with `status: "unavailable"`. That
contract does not change.

## Enrichment model

Optional enrichments are requested through an `include` query parameter.

```http
GET /v1/assets/{slug}?include=priceStats,priceTrend,priceSeries&quoteCurrency=USD&window=24h&granularity=1h
```

### Query parameters

| Name            | Required | Default          | Allowed values                                  | Notes |
| --------------- | -------- | ---------------- | ----------------------------------------------- | ----- |
| `include`       | No       | none             | comma-separated: `priceStats`, `priceTrend`, `priceSeries` | Unknown tokens are ignored. |
| `quoteCurrency` | No       | `USD`            | `USD`, `MXN`, `USDC`, `BTC`                      | Applied to requested enrichments. |
| `window`        | No       | `24h`            | `1h`, `24h`, `7d`, `30d`                         | Applied to requested enrichments. |
| `granularity`   | No       | upstream default | `5m`, `1h`, `1d`                                 | Forwarded only when provided. |

Recommended `include` values:

- `priceStats`
- `priceTrend`
- `priceSeries`

Rules:

- When `include` is absent, the endpoint returns the existing stable shape plus
  the latest price block, with no signal enrichment.
- `include` tokens are matched case-insensitively after trimming. Unknown
  tokens are ignored rather than rejected, consistent with the Mother API
  convention that unknown query parameters are ignored.
- `quoteCurrency`, `window`, and `granularity` only take effect when at least
  one enrichment is requested. They follow the same allowed values and
  forwarding rules as `SPEC-003` and must obey ADR-001.

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
| `priceSeries` | `GET /prices/series`  | UI/demo charting series; pass-through of upstream `points` and `meta`. Uses the same `window`/`granularity` model and obeys ADR-001. |

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
the existing Mother API snake_case envelope style (`chain_maps`, `asset_id`,
`quote_currency`).

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
  "chain_maps": [],
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

- `signals` is present only when `include` requested at least one enrichment.
- Each requested enrichment appears as a key under `signals`
  (`price_stats`, `price_trend`, `price_series`). On success the value is the
  pass-through upstream payload; on failure the value is `null` and a
  corresponding entry is added to `enrichment_errors`.
- `enrichment_errors` is present only when `include` requested at least one
  enrichment. It is an empty array when all requested enrichments succeed.
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

The implementation PR must update `CONTRACTS.md` with any new public fields and
enrichment error codes introduced by this endpoint.

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

## Implementation notes

- Extend the existing [src/price_indexer/](../../src/price_indexer/) client. Do
  not create a second client, token convention, timeout setting, or base URL.
- Stats and trend request helpers are shared with `SPEC-003`. This spec is a
  second consumer of the same typed helpers; the asset service composes them.
- This spec also adds a typed `GET /prices/series` request helper to the same
  client, because the direct MCP/agent series endpoint in `SPEC-003` remains
  future/optional and does not provide one. The helper obeys ADR-001
  (`window`/`granularity`, never `range`/`resolution`).
- The asset-detail handler lives in the existing assets routing family
  ([src/routes/assets.rs](../../src/routes/assets.rs),
  [src/assets/service.rs](../../src/assets/service.rs)).
- Structs parsing upstream successful responses must avoid
  `deny_unknown_fields`, because `price-indexer` may add informational fields
  without a contract break.
- Enrichment lookups should be independent: one failing enrichment must not
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

## Open questions

- Whether the `priceSeries` enrichment should pass through the full upstream
  `points` array or apply a bounded shape for charting. V0 ships `priceSeries`
  and passes upstream `points` and `meta` through; any bounding is a
  presentation refinement, not a reason to defer the enrichment.
- Whether enrichment query parameters should apply uniformly to all requested
  enrichments, or whether per-enrichment overrides are ever needed. V0 assumes
  uniform `quoteCurrency`/`window`/`granularity`.

## Tests and definition of done

The implementation is complete when tests prove:

- Existing `/v1/assets/{slug}` behavior is unchanged when `include` is absent.
- Latest price enrichment is always attempted and unchanged.
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
