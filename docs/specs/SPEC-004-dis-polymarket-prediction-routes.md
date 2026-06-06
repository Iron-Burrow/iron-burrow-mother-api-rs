---
status: accepted
owner: iron-burrow
last_reviewed: 2026-06-06
agent_edit_policy: update_when_relevant
external_contract: iron-burrow-defi-intelligence-service/CONTRACTS.md@2026-06-03
---

# SPEC-004: Mother API FIFA World Cup Prediction Routes via DIS

DIS-backed Mother API public/demo routes that expose FIFA World Cup 2026
prediction snapshots for the hackathon demo.

This spec defines **Mother API** behavior only. The two routes below are
Mother API routes. They are thin, public-facing wrappers over a DIS-owned
internal resolver. Mother API does not call Polymarket directly, and it does
not own provider resolution, Gamma/CLOB parsing, probability normalization, or
any prediction-market storage.

## 1. Status

Accepted and implemented. The binding public route contract now lives in
[CONTRACTS.md](../../CONTRACTS.md); this spec records the implementation
intent and ownership boundaries behind that contract.

This spec is a sibling of [SPEC-001](SPEC-001-dis-aave-v3-realized-yield.md).
Both describe Mother API → DIS integrations for different features:

- SPEC-001: Mother API → DIS Aave V3 realized yield.
- SPEC-004: Mother API → DIS Polymarket World Cup predictions.

SPEC-004 reuses the DIS client foundation introduced by SPEC-001 (the
`src/dis/` module, `DIS_BASE_URL` configuration, and the optional-config +
graceful-degradation pattern). It is otherwise an independent integration and
does not extend or modify the Aave work.

Implementation note: Mother API owns only the public/demo route wrapper shape.
DIS owns Polymarket access, provider parsing, provider normalization, and
probability interpretation. Mother API must never call Polymarket directly.
The implemented public route promises are documented in `CONTRACTS.md`.

## 2. Summary

Mother API exposes two public/demo-facing prediction routes:

```http
GET /v1/predictions/fifa-world-cup/winner
GET /v1/predictions/fifa-world-cup/{country}
```

Each Mother API route is a thin facade over the DIS internal resolver:

```text
Sentinel / Maria UI
  → Mother API public prediction route   (this spec)
  → DIS internal Polymarket snapshot endpoint
  → Polymarket public market-data APIs
```

Ownership split:

- **DIS** owns the internal resolver endpoint
  `POST /internal/v1/prediction-markets/polymarket/snapshot`, plus all
  Polymarket access, parsing, and probability normalization.
- **Mother API** owns the public route shape, path-parameter normalization,
  the DIS call, response simplification, and a sanitized public error policy.

## 3. Goals and non-goals

Goals:

- Add demo-friendly public Mother API routes for World Cup prediction
  snapshots.
- Reuse the SPEC-001 DIS client foundation; keep all Polymarket logic in DIS.
- Preserve DIS source/determinism metadata in the public response.
- Keep the Mother API implementation intentionally thin for the hackathon.

Non-goals (out of scope for this spec):

- Calling Polymarket Gamma or CLOB directly from Mother API.
- Market discovery, watchlists, historical snapshots, or trading.
- A prediction database table, read-model cache, worker, or wallet.
- A generic `/v1/predictions/{provider}/...` abstraction.
- Any DIS contract change.
- Future `provider_event` compatibility or a coordinated DIS schema migration.

## 4. Public routes (Mother API)

Mother API calls DIS for every request. It never calls Polymarket directly.

```text
GET /v1/predictions/fifa-world-cup/winner
  → POST {DIS_BASE_URL}/internal/v1/prediction-markets/polymarket/snapshot
     { "event_slug": "fifa-world-cup-2026-winner" }

GET /v1/predictions/fifa-world-cup/{country}
  → POST {DIS_BASE_URL}/internal/v1/prediction-markets/polymarket/snapshot
     { "event_slug": "fifa-world-cup-2026-country-probability",
       "country": "{country}" }
```

### 4.1 `GET /v1/predictions/fifa-world-cup/winner`

Returns a live Polymarket-implied snapshot of the 2026 World Cup winner market.
No query parameters; unknown query parameters are ignored.

Success — `200 OK`:

```json
{
  "ok": true,
  "event": "2026 FIFA World Cup Winner",
  "event_slug": "fifa-world-cup-2026-winner",
  "odds": [
    { "team": "France", "probability": "0.18", "price": "0.18", "currency": "USDC" },
    { "team": "Spain", "probability": "0.17", "price": "0.17", "currency": "USDC" }
  ],
  "source": "polymarket",
  "deterministic": true,
  "captured_at": "2026-06-03T18:20:00Z"
}
```

### 4.2 `GET /v1/predictions/fifa-world-cup/{country}`

Returns a live Polymarket-implied probability for a configured World Cup
country demo market, for example `GET /v1/predictions/fifa-world-cup/mexico`.

Path parameter:

| Field     | Type   | Required | Notes                                                                                       |
| --------- | ------ | -------- | ------------------------------------------------------------------------------------------- |
| `country` | string | yes      | Case-insensitive country slug. DIS is the source of truth for the supported demo countries. |

Mother API normalizes the path parameter (trim, lowercase) before sending it to
DIS as `country`. DIS validates whether the country is supported.

Success — `200 OK`:

```json
{
  "ok": true,
  "market": "Mexico to reach Round of 16",
  "country": { "slug": "mexico", "name": "Mexico" },
  "probability": "0.63",
  "price": "0.63",
  "currency": "USDC",
  "source": "polymarket",
  "deterministic": true,
  "captured_at": "2026-06-03T18:20:00Z"
}
```

Mother API does not expose DIS-internal fields such as `provider_market` in the
public response. Those remain in DIS logs and the DIS contract.

### 4.3 Deployed DIS success translation

Mother API decodes the currently deployed DIS success shapes directly. DIS
success responses do not contain Mother API's public `ok`, `event`, `odds`, or
`country` fields.

For winner responses, Mother API maps:

- DIS `event_title` to public `event`.
- DIS `outcomes[].name` to public `odds[].team`.
- DIS outcome probability, price, and currency strings without conversion.

For country responses, Mother API maps:

- DIS `subject.slug` and `subject.name` to public `country`.
- DIS `market`, `probability`, `price`, and `currency` directly.

Both response variants preserve DIS `source`, `deterministic`, and
`captured_at`. Mother API ignores `provider_market`, `warnings`, `source_kind`,
`mode`, provider URLs, provider IDs, and provider condition IDs. Support for a
future `provider_event` field is deferred to a separate change.

## 5. Decimal policy

Mother API preserves DIS probability and price values as JSON **strings** and
must not convert them to floating-point numbers. This matches the existing
`CONTRACTS.md` convention that decimal-typed values are returned as strings.
Affected fields: `odds[].probability`, `odds[].price`, `probability`, `price`.
Any number conversion belongs at the UI edge, not in the Mother API contract.

## 6. Error policy

Mother API hides DIS and Polymarket complexity from end users. It does **not**
pass DIS error codes, provider payloads, transport internals, or a `details`
object through to clients. Public errors use Mother API's existing static
error envelope (`src/error.rs`): a fixed `code` and `message` per the standard
`{ "ok": false, "error": { "code", "message" } }` shape.

```json
{
  "ok": false,
  "error": {
    "code": "unsupported_prediction_subject",
    "message": "Prediction subject is not supported for this event."
  }
}
```

DIS outcomes are mapped to this small, stable set of Mother API codes:

| Condition                                   | HTTP | Public code                       |
| ------------------------------------------- | ---: | --------------------------------- |
| Country missing or unsupported by DIS       |  400 | `unsupported_prediction_subject`  |
| Prediction provider unavailable / failed    |  503 | `prediction_provider_unavailable` |
| Prediction provider timed out               |  504 | `prediction_provider_timeout`     |
| DIS unconfigured, unreachable, or reports an availability failure | 503 | `prediction_resolver_unavailable` |
| Mother times out waiting for DIS | 504 | `prediction_resolver_timeout` |
| HTTP 200 success body does not match the expected winner/country shape | 502 | `prediction_resolver_schema_mismatch` |
| Non-success DIS body is not a valid error envelope | 502 | `prediction_resolver_malformed_response` |
| DIS returns `internal_error` or an unknown code in a valid error envelope | 502 | `prediction_resolver_error` |
| Any other unexpected Mother API failure     |  500 | `internal_error`                  |

Rationale: an unsupported country is user/demo input and maps to `400`.
Schema incompatibility, malformed error envelopes, and unknown structured
errors are distinct from availability because Mother reached DIS and received
a response. Public errors remain sanitized and do not expose DIS messages,
provider data, or response bodies.

## 7. DIS client (Mother API side)

This spec extends the DIS client foundation from
[SPEC-001](SPEC-001-dis-aave-v3-realized-yield.md) with a single
prediction-snapshot method. It does not introduce new configuration; it reuses
the SPEC-001 variables (`DIS_BASE_URL`, `DIS_REQUEST_TIMEOUT_MS`,
`DIS_RETRY_MAX_ATTEMPTS`) and the same optional-config + graceful-degradation
behavior. If `DIS_BASE_URL` is unset, the DIS client is disabled and the
prediction routes return `prediction_resolver_unavailable` rather than failing
to start.

Suggested method shape:

```rust
async fn get_polymarket_prediction_snapshot(
    &self,
    request: PolymarketSnapshotRequest,
) -> Result<PolymarketSnapshotResponse, DisClientError>;

struct PolymarketSnapshotRequest {
    event_slug: String,
    country: Option<String>,
}

enum PolymarketSnapshotResponse {
    Winner(PolymarketWinnerSnapshot),
    Country(PolymarketCountrySnapshot),
}
```

The client must:

- `POST /internal/v1/prediction-markets/polymarket/snapshot` as JSON using the
  configured DIS base URL and timeout.
- Decode DIS success and DIS error responses.
- Select and require the winner or country success shape from the request.
- Classify transport, timeout, success-schema, malformed-error-envelope, known
  error-code, and unknown-error-code outcomes separately.
- Classify HTTP success responses that cannot be deserialized as
  `UnsupportedResponseSchema`, mapped publicly to
  `prediction_resolver_schema_mismatch`.
- Classify invalid DIS error envelopes as `MalformedErrorResponse` and valid
  envelopes with unknown codes as `UnknownResolverErrorCode`; neither is
  resolver unavailability or a success-schema mismatch.
- Log response classification failures with the DIS path, status, event slug,
  expected response variant, issue/category, body length, and capped top-level
  field names. Unknown error codes may be logged only as a capped label.
- Avoid logging full provider payloads or large response bodies.
- Never log raw response bodies, provider URLs, provider IDs, or provider
  condition IDs.

The client must not call Polymarket directly, parse Gamma/CLOB, interpret market
prices, or retry aggressively.

## 8. Implementation surface (Mother API)

- A new `src/routes/predictions.rs` module holds the two route handlers.
- The routes are nested under `/v1` in the router in `src/app.rs`, alongside the
  existing asset and resolve routes.
- The DIS client handle is added to `AppState` (`src/state.rs`), mirroring how
  `price_indexer_client` is held today, so handlers can reach DIS.

These routes are implemented and covered by `CONTRACTS.md`.
