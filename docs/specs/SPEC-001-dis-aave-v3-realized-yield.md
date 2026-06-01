---
status: draft
owner: iron-burrow
last_reviewed: 2026-06-01
agent_edit_policy: update_when_relevant
external_contract: iron-burrow-defi-intelligence-service/CONTRACTS.md@2026-06-01
---

# SPEC-001 - DIS Aave V3 Realized Yield Client

Mother API internal client for the `iron-burrow-defi-intelligence-service`
(DIS) Aave V3 realized yield resolver.

This spec defines the Mother API → DIS client contract only. It does not
define a Mother API public endpoint that wraps DIS, and it does not authorize
Mother API to reimplement any DIS-owned behavior.

## Purpose

Mother API may need deterministic Aave V3 realized yield data. DIS owns that
responsibility. Mother API must call DIS instead of reimplementing:

- Aave V3 income index math.
- Aave market and reserve lookup.
- Bigwig archive RPC calls.
- Historical block finality checks.
- Block timestamp lookup.
- DIS-specific validation rules.

Mother API owns only client configuration, request construction, response
parsing, error classification, retry behavior, and eventual public wrapping
through a separate spec and contract update.

## Scope

Mother API will implement a typed, internal-only HTTP client for one DIS
endpoint:

```http
POST /internal/v1/aave/v3/yield/realized
```

The authoritative DIS contract is `CONTRACTS.md` in the
`iron-burrow-defi-intelligence-service` repository, last reviewed
`2026-06-01`. This spec pins to that revision. Behavior changes in DIS
require a corresponding revision of this spec.

Phase 1 limits Mother API requests to:

- `chain_id = 1` (Ethereum mainnet).
- `market = "aave-v3-ethereum"`.
- `asset` values that DIS accepts as asset symbols or asset slugs, for
  example `"USDC"`.

The client lives at `src/dis/` (planned: `src/dis/mod.rs`,
`src/dis/client.rs`), mirroring the existing
[src/price_indexer/](../../src/price_indexer/) layout.

## Non-goals

This spec explicitly does **not** cover:

- A public Mother API endpoint wrapping the DIS resolver. Such an endpoint
  requires its own accepted RFC or spec and a public contract entry.
- Aave V3 income index math.
- Direct Bigwig archive RPC calls from Mother API.
- Raw Aave event indexing or holder indexing.
- Aave market/reserve configuration lookup outside DIS.
- Reserve enablement checks.
- Historical block finality checks.
- Block timestamp lookup.
- Any field, response shape, or guarantee not promised by the DIS
  `CONTRACTS.md@2026-06-01`.
- Coupling to DIS Python implementation details (exception messages,
  internal class names, FastAPI handler internals).
- Caching.
- Billing, quotas, rate limiting, auth, or x402 boundaries.

## DIS endpoint dependency

| Property        | Value                                                  |
| --------------- | ------------------------------------------------------ |
| Method          | `POST`                                                 |
| Path            | `/internal/v1/aave/v3/yield/realized`                  |
| Network         | `iron-burrow-net` Docker network, internal only        |
| Public ingress  | None. Caddy does not expose DIS.                       |
| Transport       | HTTP/1.1, JSON request/response                        |
| Auth            | None today. DIS does not authenticate callers.         |
| Tracing headers | DIS does not currently interpret `x-request-id`.       |

Mother API may surface DIS-derived data publicly only through a separately
specified public endpoint and `CONTRACTS.md` update. This client alone creates
no public promise.

## Configuration

Mother API follows the optional-config + graceful-degradation pattern
already established for `PRICE_INDEXER_URL` in [src/config.rs](../../src/config.rs).

| Variable                 | Default | Description                                                                                      |
| ------------------------ | ------- | ------------------------------------------------------------------------------------------------ |
| `DIS_BASE_URL`           | unset   | DIS internal base URL on `iron-burrow-net`, for example `http://defi-intelligence-service:8080`. |
| `DIS_REQUEST_TIMEOUT_MS` | `5000`  | Per-attempt HTTP timeout in milliseconds.                                                        |
| `DIS_RETRY_MAX_ATTEMPTS` | `2`     | Maximum total HTTP attempts per logical call (initial + retries). `1` disables retries.          |

Behavior:

- If `DIS_BASE_URL` is unset, the DIS client is **disabled**. Mother API
  starts normally. Any code path that requests Aave V3 realized yield from
  DIS returns a typed "DIS unavailable" result rather than panicking or
  refusing to start. This matches the price-indexer behavior in
  [src/price_indexer/client.rs](../../src/price_indexer/client.rs).
- Invalid values for `DIS_REQUEST_TIMEOUT_MS` or `DIS_RETRY_MAX_ATTEMPTS`
  cause a typed startup error, consistent with how
  `PRICE_INDEXER_TIMEOUT_MS` is parsed today.

`.env.example` and `.env.production.example` will be updated in the
implementation PR; this spec does not yet edit them.

## Typed request model

Rust shape (illustrative; not yet implemented):

```rust
#[derive(Debug, Serialize)]
struct RealizedYieldRequest<'a> {
    chain_id: u64,
    market: &'a str,
    asset: &'a str,
    from_block: u64,
    to_block: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    include_annualized_apy_estimate: Option<bool>,
}
```

Rules:

- All fields use serde field names that match the DIS contract literally.
  No `rename_all`.
- `include_annualized_apy_estimate` is `Option<bool>`. The default at the
  call site is `Some(true)`. `None` means "use DIS default" and is allowed
  but should be rare; the default in this codebase is `true`.
- Mother API **never serializes extra fields**. DIS rejects unknown fields
  with HTTP 422. The struct above does not have a catch-all.
- `from_block` and `to_block` must satisfy `0 < from_block < to_block`
  before sending. Caller-side validation surfaces a `CallerInput` error
  without making a network call.
- Phase 1: `chain_id = 1`, `market = "aave-v3-ethereum"`. Other values are
  rejected client-side until DIS expands support.
- `asset` is passed through as configured (e.g., `"USDC"`). Mother API does
  not normalize case or rewrite to addresses.

## Typed response model

Rust shape (illustrative):

```rust
#[derive(Debug, Deserialize)]
struct RealizedYieldResponse {
    protocol: String,                       // always "aave-v3"
    chain_id: u64,
    market: String,
    asset: String,
    underlying_asset_address: String,
    from_block: u64,
    to_block: u64,
    from_block_timestamp: Option<String>,   // ISO-8601 Z
    to_block_timestamp: Option<String>,
    from_income_index_ray: String,          // decimal-safe, raw uint256
    to_income_index_ray: String,
    realized_yield: String,                 // decimal-safe; may be negative
    annualized_apy_estimate: Option<String>,
    annualized_apy_estimate_basis: Option<String>,
    source: String,                         // always "bigwig_archive_eth_call"
    verification: String,                   // always "deterministic_onchain"
    confidence: String,                     // "verified" | "degraded"
    computed_at: String,                    // ISO-8601 Z
    warnings: Vec<String>,                  // present, possibly empty
}
```

Rules:

- Mother API does **not** use `#[serde(deny_unknown_fields)]` on the
  response. New informational fields added by DIS must not break Mother API.
- Mother API does not assume key order.
- The `protocol`, `source`, and `verification` literal values are not
  re-validated in the type itself, but a logged warning fires if they ever
  differ from the contract literals. This is observability, not a hard
  failure.
- `confidence` is parsed as a string and mapped to an internal enum
  `{Verified, Degraded, Unknown(String)}`. `Unknown` is logged and treated
  as degraded for caller-facing purposes.
- `annualized_apy_estimate` is `Some` only when DIS could resolve block
  timestamps and the request did not opt out. `annualized_apy_estimate_basis`
  is `Some("block_timestamp_interval")` when APY is `Some`, else `None`.

### Warning catalogue

Documented codes from the contract:

| Code                       | Meaning                                                                                    |
| -------------------------- | ------------------------------------------------------------------------------------------ |
| `decreasing_income_index`  | `to_income_index < from_income_index`. `realized_yield` may be negative. APY still valid.  |
| `timestamp_lookup_failed`  | Block timestamps could not be resolved. APY and APY basis are `null`. Yield is returned.   |

Unknown warning codes are tolerated: surfaced via tracing and returned
verbatim to in-process callers. They never cause the response to be rejected.

## Decimal parsing policy

All monetary, ratio, APY, and ray-index values stay as `String` end-to-end:

- `from_income_index_ray`
- `to_income_index_ray`
- `realized_yield`
- `annualized_apy_estimate` (when present)

Forbidden in this client and any code consuming its response:

- `f32`, `f64`, or any IEEE-754 conversion of the above fields.
- Lossy parses (`parse::<f64>()`, `as f64`, etc.).
- String trimming or reformatting that changes representation.

Allowed:

- Round-trip serialization of the same `String` to downstream callers.
- Parsing into a decimal/big-integer-safe type at the consumer (not in this
  client) once such a dependency is introduced and accepted in a future
  spec. Candidate crates (`rust_decimal`, `num-bigint`) are noted but not
  pulled in by this client.

## Error mapping strategy

DIS exposes a deterministic error envelope:

```json
{
  "error": {
    "code": "unsupported_asset",
    "message": "...",
    "details": { "...": "..." }
  }
}
```

Mother API parses `error.code` (string) and the HTTP status. `error.message`
is logged but **never propagated to a public Mother API caller**.
`error.details` is treated as `serde_json::Value` and surfaced only to
internal callers and logs; keys are resolver-specific and may change.

Mother API defines a typed error enum:

```rust
enum DisErrorCode {
    UnsupportedChain,
    UnsupportedMarket,
    UnsupportedAsset,
    InvalidBlockRange,
    BlockNotFinal,
    BigwigRateLimited,
    BigwigProviderUnavailable,
    BigwigProviderTimeout,
    BigwigBadRequest,
    BigwigCapabilitiesUnavailable,
    AaveCallFailed,
    TimestampLookupFailed,
    InvalidIncomeIndex,
    CalculationFailed,
    InternalError,
    Unknown(String),
}
```

Mapping table (from DIS `CONTRACTS.md@2026-06-01`):

| DIS code                          | HTTP | Mother API category   | Retry? |
| --------------------------------- | ---- | --------------------- | ------ |
| `unsupported_chain`               | 400  | CallerInput           | No     |
| `unsupported_market`              | 400  | CallerInput           | No     |
| `unsupported_asset`               | 404  | CallerInput           | No     |
| `invalid_block_range`             | 400  | CallerInput           | No     |
| `block_not_final`                 | 400  | CallerInput / Timing  | No     |
| `bigwig_rate_limited`             | 503  | UpstreamProvider      | Yes    |
| `bigwig_provider_unavailable`     | 503  | UpstreamProvider      | Yes    |
| `bigwig_provider_timeout`         | 504  | UpstreamProvider      | Yes    |
| `bigwig_bad_request`              | 502  | InternalDependency    | No     |
| `bigwig_capabilities_unavailable` | 503  | DependencyReadiness   | Yes    |
| `aave_call_failed`                | 502  | UpstreamProvider      | At most 1 |
| `timestamp_lookup_failed`         | 502  | UpstreamProvider      | At most 1 |
| `invalid_income_index`            | 502  | UpstreamProtocolData  | No     |
| `calculation_failed`              | 500  | DisInternal           | No     |
| `internal_error`                  | 500  | DisInternal           | At most 1 |

Separate path for FastAPI validation errors:

- HTTP `422` from DIS uses the FastAPI default `{"detail": [...]}` body,
  not the deterministic envelope. Mother API treats any `422` as a
  `RequestConstructionBug` category, logs the response body verbatim, and
  never retries. A `422` from this client is a Mother API bug, because
  Mother API controls the request shape.

Other surprises:

- Non-2xx status with a body that fails to decode as either envelope:
  category `MalformedDisResponse`, no retry.
- Transport errors (DNS, connection refused, TLS, read timeout): category
  `Transport`, eligible for one bounded retry under the same budget as
  retryable 5xx.

## Retry/timeout policy

- Per-attempt timeout: `DIS_REQUEST_TIMEOUT_MS` (default `5000`).
- Maximum attempts: `DIS_RETRY_MAX_ATTEMPTS` (default `2`). `1` disables
  retries entirely.
- Backoff: bounded and short. The implementation may choose fixed or
  exponential backoff, but sleep between attempts must be capped at `1000 ms`
  and must not require a new production dependency solely for jitter.
- Retryable conditions: transport errors, HTTP `503`, HTTP `504`, and the
  DIS codes `bigwig_rate_limited`, `bigwig_provider_unavailable`,
  `bigwig_provider_timeout`, `bigwig_capabilities_unavailable`,
  `aave_call_failed`, `timestamp_lookup_failed`, `internal_error`. The
  last three retry **at most once** regardless of the configured budget.
- Never retried: HTTP `4xx` (including `422`), `bigwig_bad_request`,
  `invalid_income_index`, `calculation_failed`, `MalformedDisResponse`,
  any client-side validation failure.
- The DIS endpoint is read-only and deterministic for a given input. Retry
  is safe.
- Total wall-clock time per logical call is bounded by
  `attempts * (per_attempt_timeout + max_backoff)`. The client does not need a
  separate global deadline beyond the attempt loop.

## Observability

The client emits structured `tracing` events on every attempt and on the
final outcome. Field names (planned):

- `dis.endpoint` — `"aave_v3_realized_yield"`.
- `dis.attempt` — 1-based attempt number.
- `dis.attempts_max` — configured maximum.
- `dis.http_status` — when an HTTP response was received.
- `dis.error_code` — DIS deterministic `error.code` when present.
- `dis.warning_codes` — comma-joined warning codes on success.
- `dis.confidence` — `"verified"` or `"degraded"` on success.
- `dis.duration_ms` — per-attempt duration.
- `dis.request_id` — correlation ID sent on the request.
- `dis.retryable` — boolean classification used for the retry decision.

Correlation:

- If the inbound axum request carries `x-request-id`, the client forwards
  the same value on the outbound DIS request under the same header.
- If no inbound ID is present, the client may generate a fresh request ID
  using existing dependencies and use it for both the outbound header and log
  correlation.
- DIS does not currently interpret this header. Forwarding it now is a
  cheap forward-compatibility choice and aids log correlation today.

Logging hygiene:

- Request and response bodies are not logged at `INFO`. At `DEBUG`, the
  request body is logged with field-level care; the response body is
  summarized (status, error code, warning codes, confidence).
- Secrets and future auth tokens must never be logged.
- DIS `error.message` is logged but never returned to public callers.

## Tests

The implementation PR must include tests covering:

1. **Request construction.** Serialized body for a representative request
   matches the DIS contract byte-for-byte, including field names and the
   absence of any unknown fields.
2. **`include_annualized_apy_estimate` default.** Mother API call sites
   default to `true`; verify the serialized field is present and `true`.
3. **Success response parsing.** A canonical success payload deserializes
   into the typed response with all fields populated.
4. **Decimal preservation.** `from_income_index_ray`, `to_income_index_ray`,
   `realized_yield`, and `annualized_apy_estimate` round-trip as `String`
   without any floating-point step. Test asserts that the original literal
   is preserved exactly.
5. **Negative realized yield.** `realized_yield = "-0.0001"` plus
   `warnings = ["decreasing_income_index"]` parses, `confidence = "degraded"`.
6. **Null APY.** `annualized_apy_estimate = null` and
   `annualized_apy_estimate_basis = null` parse correctly.
7. **Unknown warning code.** A warning code not in the catalogue is
   preserved verbatim in the response and logged; the call still succeeds.
8. **Error envelope mapping.** One test per known DIS error code asserts
   the correct `DisErrorCode` variant, HTTP status, category, and
   retryability classification.
9. **FastAPI 422 path.** A `422` with the FastAPI `{"detail": [...]}` body
   is classified as `RequestConstructionBug`, not as a deterministic
   envelope, and is never retried.
10. **Timeout behavior.** A request that exceeds
    `DIS_REQUEST_TIMEOUT_MS` is reported as a transport timeout and is
    eligible for retry within the configured budget.
11. **Retry budget.** With `DIS_RETRY_MAX_ATTEMPTS = 2`, two `503`
    `bigwig_provider_unavailable` responses produce exactly two attempts,
    then surface the upstream error. With `= 1`, exactly one attempt.
12. **Non-retry on `bigwig_bad_request`.** A `502` with code
    `bigwig_bad_request` is never retried even when budget allows.
13. **Missing config disables the client.** With `DIS_BASE_URL` unset,
    service startup succeeds and the typed "DIS unavailable" path is
    returned without any network call.
14. **Correlation header forwarding.** Inbound `x-request-id` is forwarded
    verbatim; absent inbound ID, a uuid v4 is generated.
15. **Anti-reimplementation guard.** A compile-time or test-time assertion
    that the Mother API crate does not depend on Aave, Bigwig, JSON-RPC
    archive, or ethereum event-decoding crates. This is a structural test
    against `Cargo.toml`.

## Acceptance criteria

This spec is satisfied when Mother API has:

- An optional DIS client configured by `DIS_BASE_URL`.
- Typed request and response models for
  `POST /internal/v1/aave/v3/yield/realized`.
- Client-side validation for Phase 1 chain, market, and block range.
- Decimal-safe string preservation for ray, yield, and APY values.
- Typed DIS error mapping and bounded retry/timeout behavior.
- Structured tracing/logging for attempts, outcomes, warning codes, and
  retry decisions.
- Tests covering the contract above.
- No public endpoint, public response shape, Aave math, Bigwig logic,
  indexing, auth, billing, rate limiting, or in-process response caching
  added to Mother API.

## Rollout plan

- **Phase A** — this spec is reviewed and merged. No code changes.
- **Phase B** — implement `src/dis/client.rs` and `src/dis/mod.rs` behind
  the optional config above. Client is **not wired into any public route**.
  Tests from the section above ship in the same PR. Compose files and
  `.env.example` get `DIS_BASE_URL` added with a commented default.
- **Phase C** — internal-only validation against a running DIS instance on
  `iron-burrow-net`, executed from an integration test or a one-shot
  binary. Mother API public surface is unchanged.
- **Phase D** — a separate Mother API spec may define a public endpoint that
  consumes this client. That spec will own the public response shape,
  access policy, caching decision, and `CONTRACTS.md` updates. Until then,
  the client exists as a vetted internal capability.

## Open questions

These are owned by the Mother API roadmap, not by DIS, and are intentionally
left out of this spec:

1. Shape and path of the future public Mother API endpoint that wraps this
   client.
2. Whether responses are cached, and if so at what layer
   (Mother API explicitly does not do in-process response caching today).
3. Access policy for any future public wrapper.
4. Whether other DIS resolvers (non-Aave, non-Ethereum) should reuse this
   client unchanged or be modeled as separate clients per protocol.
