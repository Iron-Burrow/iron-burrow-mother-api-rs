---
status: draft
owner: iron-burrow
last_reviewed: 2026-06-02
agent_edit_policy: update_when_relevant
external_contract: iron-burrow-price-indexer/CONTRACTS.md@2026-06-02
---

# SPEC-003 - Price Signal Endpoints for MCP and Agents

Low-level, precise price signal endpoints for MCP tools and AI agents, backed
by the `iron-burrow-price-indexer` Query Layer.

This spec was split out of the original `SPEC-002` draft, which mixed two
distinct consumers. The UI/demo asset-page enrichment use case now lives in
`SPEC-002 - Asset Detail Enrichment for UI and Demo`. This spec covers only
the dedicated `/v1/assets/{slug}/signal/*` endpoints for agents.

This spec defines the Mother API -> price-indexer integration boundary only.
It does not authorize Mother API to re-own, recalculate, or reinterpret
price-derived intelligence. `price-indexer` owns price observations,
bucketization, statistics, trend formulas, confidence, and warning semantics.

## Purpose

MCP tools and agents need to request a single, specific signal without loading
a full asset page. These endpoints let a caller ask exactly one question and
receive exactly one deterministic answer:

- How did an asset move over the last 24 hours?
- What was its observed price range and coverage?
- Is the deterministic trend up, down, or flat?

Unlike asset-page enrichment (`SPEC-002`), these endpoints are precise and
strict: if the requested signal cannot be fetched, the request fails rather
than degrading to a partial response.

The source of those answers is `price-indexer`, not Mother API. Mother API
owns public routing, parameter validation, upstream orchestration, response
shape, and error mapping. DIS is not involved in this workflow.

The upstream source documents for this spec are:

- `iron-burrow-price-indexer/CONTRACTS.md@2026-06-02`.
- `iron-burrow-price-indexer/docs/rfcs/RFC-003-deterministic-price-stats-and-trend-signals.md`.
- `iron-burrow-price-indexer/docs/adr/ADR-001-unify-price-series-on-window-granularity.md`.

## Scope

Mother API will expose two planned public signal endpoints:

```http
GET /v1/assets/{slug}/signal/price-stats
GET /v1/assets/{slug}/signal/price-trend
```

These endpoints call the existing private price-indexer endpoints:

```http
GET /prices/stats
GET /prices/trend
```

A direct time-series endpoint is a future consideration:

```http
GET /v1/assets/{slug}/prices/series
```

If included, it would consume the upstream `GET /prices/series` endpoint.
This endpoint is **out of scope for V0** unless implementation scope clearly
allows it; the same parameter-mapping and error-mapping doctrine in this spec
would apply, and the upstream `meta`/`points` shape would pass through. It is
documented here only so agents do not invent a different path or parameter
model later.

This spec describes the intended public surface. It does not itself update
`CONTRACTS.md`; the implementation PR that adds these endpoints must update
the Mother API contract in the same change.

## Ownership

`price-indexer` owns:

- Price observations and quote-currency derivation.
- The canonical `window` and `granularity` series model.
- Bucket anchoring, bucket statuses, and coverage metadata.
- Stats formulas and deterministic formatting.
- Trend formulas, direction, confidence, and warnings.
- Error codes and validation for the private upstream endpoints.

Mother API owns:

- The public `/v1/assets/{slug}/signal/*` API surface.
- Public parameter names, defaults, and validation.
- Calling `price-indexer` with internal bearer authentication.
- Mapping upstream errors into Mother API's error envelope.
- Preserving upstream successful response fields and warnings.
- Future auth, rate-limit, quota, and billing policy when those foundations
  exist in Mother API.

Mother API must not:

- Recalculate statistics or trend locally.
- Re-bucket price series locally.
- Reinterpret, hide, or invent upstream warning codes.
- Call DIS for price stats or price trend.
- Send legacy `range` or `resolution` parameters to `price-indexer`.

## Upstream price-indexer dependency

| Property       | Value                                          |
| -------------- | ---------------------------------------------- |
| Service        | `iron-burrow-price-indexer`                    |
| Auth           | `Authorization: Bearer <PRICE_QL_INTERNAL_TOKEN>` |
| Transport      | HTTP/1.1, JSON request/response                |
| Stats path     | `GET /prices/stats`                            |
| Trend path     | `GET /prices/trend`                            |
| Error envelope | `{ "error": { "code": "...", "message": "..." } }` |

Shared upstream parameters:

| Name            | Required | Default | Allowed values              | Notes |
| --------------- | -------- | ------- | --------------------------- | ----- |
| `slug`          | Yes      | None    | canonical asset slug        | Trimmed and lowercased by `price-indexer`. |
| `quoteCurrency` | No       | `USD`   | `USD`, `MXN`, `USDC`, `BTC` | Trimmed and uppercased by `price-indexer`. |
| `window`        | Yes      | None    | `1h`, `24h`, `7d`, `30d`   | Mother API defaults this before forwarding. |
| `granularity`  | No       | per window | `5m`, `1h`, `1d`         | Forward only when the caller provides it. |
| `asOf`          | No       | None    | ISO-8601 with timezone      | Supported upstream, not exposed by Mother API V0. |

Allowed upstream `window` and `granularity` combinations:

| `window` | Default `granularity` | Allowed granularities |
| -------- | --------------------- | --------------------- |
| `1h`     | `5m`                  | `5m`                  |
| `24h`    | `1h`                  | `5m`, `1h`            |
| `7d`     | `1h`                  | `1h`                  |
| `30d`    | `1d`                  | `1d`                  |

ADR-001 makes `window` and `granularity` the current time-series doctrine.
Mother API must not use the legacy `range` or `resolution` selectors.

## Public endpoint parameters

### `GET /v1/assets/{slug}/signal/price-stats`

Path parameters:

| Name   | Required | Notes |
| ------ | -------- | ----- |
| `slug` | Yes      | Forwarded as upstream `slug`. |

Query parameters:

| Name            | Required | Default | Allowed values              |
| --------------- | -------- | ------- | --------------------------- |
| `quoteCurrency` | No       | `USD`   | `USD`, `MXN`, `USDC`, `BTC` |
| `window`        | No       | `24h`   | `1h`, `24h`, `7d`, `30d`   |
| `granularity`  | No       | upstream default | `5m`, `1h`, `1d` |

`asOf` is not exposed in V0.

### `GET /v1/assets/{slug}/signal/price-trend`

Path parameters and query parameters are identical to
`GET /v1/assets/{slug}/signal/price-stats`.

## Parameter mapping

Stats request:

```http
GET /v1/assets/{slug}/signal/price-stats?quoteCurrency=USD&window=24h&granularity=1h
```

calls:

```http
GET /prices/stats?slug={slug}&quoteCurrency=USD&window=24h&granularity=1h
```

Trend request:

```http
GET /v1/assets/{slug}/signal/price-trend?quoteCurrency=USD&window=24h&granularity=1h
```

calls:

```http
GET /prices/trend?slug={slug}&quoteCurrency=USD&window=24h&granularity=1h
```

Rules:

- The path `slug` is forwarded as upstream `slug`.
- Omitted `quoteCurrency` becomes `USD`.
- Omitted `window` becomes `24h`.
- `granularity` is forwarded only when provided by the public caller.
- Mother API validates public enum values before calling upstream.
- Mother API does not expose or forward `asOf` in V0.
- Mother API never sends `range`, `resolution`, `from`, `to`, `interval`,
  `sourceType`, `limit`, or `beforeId`.

## Response mapping

V0 should use a mostly pass-through response shape. Mother API may wrap the
response in its standard success envelope if the implementation keeps that
pattern, but it should not transform upstream price signal fields.

Mother API must preserve stats fields returned by `price-indexer`, including:

- `slug`
- `assetId`
- `quoteCurrency`
- `window`
- `granularity`
- `from`
- `to`
- `expectedBucketCount`
- `sampleCount`
- `carryForwardBucketCount`
- `missingBucketCount`
- `coverageRatio`
- `firstPrice`
- `lastPrice`
- `minPrice`
- `maxPrice`
- `meanPrice`
- `medianPrice`
- `sampleStdDev`
- `coefficientOfVariation`
- `absoluteChange`
- `percentChange`
- `minTimestamp`
- `maxTimestamp`
- `warnings`

Mother API must preserve trend fields returned by `price-indexer`, including:

- `slug`
- `assetId`
- `quoteCurrency`
- `window`
- `granularity`
- `from`
- `to`
- `expectedBucketCount`
- `sampleCount`
- `carryForwardBucketCount`
- `missingBucketCount`
- `coverageRatio`
- `firstPrice`
- `lastPrice`
- `percentChange`
- `direction`
- `slope`
- `slopeUnit`
- `rSquared`
- `confidence`
- `warnings`

Field additions from `price-indexer` are allowed. The Mother API client should
tolerate unknown successful response fields unless the implementation chooses
a raw JSON pass-through strategy.

Decimal values remain JSON strings. Mother API must not parse decimal price,
ratio, slope, or volatility fields into floating-point values.

## Warning behavior

If `price-indexer` returns HTTP 200 with `warnings`, Mother API returns HTTP
200 and preserves the warning strings exactly.

Mother API must not:

- Hide warning codes.
- Convert warnings into errors.
- Replace warning codes with Mother API-specific codes.
- Treat unknown warning codes as malformed responses.

Known upstream warning codes from `CONTRACTS.md@2026-06-02` include:

- `low_series_coverage`
- `insufficient_observed_samples`
- `missing_buckets_detected`
- `insufficient_samples_for_strong_trend`
- `one_hour_window_has_limited_statistical_power`
- `non_positive_price_detected`
- `invalid_price_detected`
- `contradictory_movement_detected`

The list is extensible. Mother API consumers must tolerate unknown warning
codes.

## Error mapping

Mother API uses its own error envelope:

```json
{
  "ok": false,
  "error": {
    "code": "price_indexer_unavailable",
    "message": "Price signals are temporarily unavailable."
  }
}
```

Upstream mapping:

| Upstream result | Mother API HTTP | Mother API `error.code` | Notes |
| --------------- | --------------- | ----------------------- | ----- |
| `400 INVALID_REQUEST` | `400` | `invalid_request` | Caller supplied an unsupported public parameter or disallowed combination. |
| `404 NOT_FOUND` | `404` | `asset_not_found` | Asset or requested price series is unavailable for the public request. |
| `401 UNAUTHORIZED` | `502` | `upstream_auth_failed` | Mother API configuration failure, not caller auth failure. |
| `500 INTERNAL_ERROR` | `502` | `price_indexer_error` | Upstream service failed while handling a valid request. |
| Timeout or connection failure | `503` | `price_indexer_unavailable` | Upstream is unavailable or unreachable. |
| Malformed success or error body | `502` | `upstream_invalid_response` | Upstream did not return the expected JSON shape. |

Implementation must not propagate upstream `error.message` verbatim to public
callers. Public messages are owned by Mother API and may be less specific than
internal logs.

The implementation PR must update `CONTRACTS.md` with any new public error
codes introduced by these endpoints.

## Configuration

The implementation must reuse the existing price-indexer client and
configuration pattern:

| Variable                   | Default | Description |
| -------------------------- | ------- | ----------- |
| `PRICE_INDEXER_URL`        | unset   | Internal price-indexer base URL, for example `http://price-indexer:3010`. |
| `PRICE_QL_INTERNAL_TOKEN`  | unset   | Bearer token sent to private price-indexer routes. |
| `PRICE_INDEXER_TIMEOUT_MS` | `2000`  | Per-request timeout in milliseconds. |

Behavior:

- If `PRICE_INDEXER_URL` or `PRICE_QL_INTERNAL_TOKEN` is unset, the
  price-indexer client remains disabled.
- Asset list and asset detail price enrichment behavior remains unchanged.
- Price signal routes should return a typed `price_indexer_unavailable`
  response when the client is disabled, rather than panicking or starting a
  second client.
- Invalid `PRICE_INDEXER_TIMEOUT_MS` remains a startup configuration error,
  matching current behavior.

## Implementation notes

Implementation should extend the existing
[src/price_indexer/](../../src/price_indexer/) client module. Do not create a
second price-indexer client, token convention, timeout setting, or base URL
configuration.

The client should add typed request helpers for:

- `GET /prices/stats`
- `GET /prices/trend`

The route layer should add handlers under the existing assets routing family,
because the public API is asset-slug centered.

Any struct used to parse upstream successful responses should avoid
`deny_unknown_fields`, because `price-indexer` may add informational fields
without a contract break.

## Non-goals

This spec explicitly does not cover:

- Implementing code in this documentation change.
- Updating `CONTRACTS.md` before routes exist.
- Asset-page enrichment, the `include` query model, or UI composition
  (owned by `SPEC-002`).
- Partial-enrichment behavior. Signal endpoints fail when the signal cannot
  be fetched.
- Public `/v1/prices/*` routes.
- Stablecoin depeg or stability endpoints.
- Read-model caching or materialized views.
- Recalculating stats or trend in Mother API.
- Changing price-indexer formulas or contracts.
- DIS integration.
- Billing, rate-limit, API key, or x402 redesign.
- Exposing upstream `asOf` in V0.
- Extending the price-indexer `window` and `granularity` matrix.
- Implementing the future `GET /v1/assets/{slug}/prices/series` endpoint in V0.

## Tests and definition of done

The implementation is complete when tests prove:

- Existing latest price enrichment remains unchanged.
- Mother API calls `/prices/stats` with the exact mapped query parameters.
- Mother API calls `/prices/trend` with the exact mapped query parameters.
- Omitted `quoteCurrency` becomes `USD`.
- Omitted `window` becomes `24h`.
- Omitted `granularity` is not sent upstream.
- `range` and `resolution` are never sent upstream.
- Upstream warnings are preserved on HTTP 200 responses.
- Upstream `400`, `401`, `404`, `500`, timeout, connection failure, and
  malformed response cases map to the Mother API errors in this spec.
- Public response shape is pinned by route tests.
- Mother API does not calculate stats or trend locally.
- Missing price-indexer configuration returns `price_indexer_unavailable` for
  signal routes without breaking existing asset endpoints.
