---
status: draft
owner: iron-burrow
last_reviewed: 2026-06-25
agent_edit_policy: update_when_relevant
external_contract: iron-burrow-defi-intelligence-service/CONTRACTS.md@2026-06-01
---

# SPEC-005 - Aave V3 Portfolio Estimate Endpoint

Public Mother API endpoint for the ETH Mexico hackathon profile of Aave V3
portfolio estimation. It answers one question:

> If I had supplied these asset quantities to Aave V3 over the supported demo
> window, what would the portfolio have been worth at the beginning and what is
> its estimated value at the end?

This is a hackathon-scoped public wrapper. It is intentionally constrained,
operates only inside a controlled demo environment, and does not define the
production architecture for portfolio simulation.

## Endpoint

```http
POST /v1/portfolio/aave-v3/estimate
```

## Dependencies

This spec composes two existing internal capabilities. It does not redefine or
reimplement either of them.

- **DIS Aave V3 realized yield** — Mother API consumes the internal DIS client
  defined by
  [SPEC-001](SPEC-001-dis-aave-v3-realized-yield.md). That client owns the DIS
  request shape, response parsing, error mapping, retry, and decimal policy for
  `POST /internal/v1/aave/v3/yield/realized`. SPEC-001 is a **prerequisite**:
  this endpoint cannot ship until the SPEC-001 Aave realized-yield client
  exists. SPEC-005 only constructs the resolved block range and consumes
  `realized_yield`.
- **Price Indexer point-in-time price (`/prices/at`)** — Mother API also
  depends on the existing internal Price Indexer point-in-time price capability
  documented by the Price Indexer contract
  (`iron-burrow-price-indexer/CONTRACTS.md@2026-06-02`) and referenced by
  [SPEC-002](SPEC-002-asset-detail-enrichment.md). This is an **internal-only**
  dependency of the portfolio estimator. Mother API does **not** expose a
  point-in-time, `asOf`, or historical price endpoint publicly in this spec.

Mother API CONTRACTS.md confirms that **no public point-in-time price surface
exists today**: asset detail does not expose `asOf`, and public price routes
outside the signal surface are out of scope. This spec does not change that.

## Ownership

Mother API owns:

- the public request and response shape;
- the supported asset allowlist and Mother-owned asset decimals;
- request and quantity validation;
- portfolio value computation in the requested quote currency;
- the fixed demo window to block-range mapping;
- portfolio value composition;
- public error mapping.

Mother API does not own:

- Aave V3 income index math or realized-yield resolution (DIS, via SPEC-001);
- Aave reserve lookup beyond the explicit public slug mapping;
- Bigwig archive RPC calls;
- generic datetime-to-block resolution;
- price observation, derivation, or historical price storage (Price Indexer);
- generic portfolio simulation.

## Scope

Phase 1 supports only:

- network: Ethereum mainnet (`network_slug = "eth-mainnet"`; DIS still uses
  EIP-155 `chain_id = 1` internally);
- protocol: Aave V3;
- market: `aave-v3-ethereum`;
- portfolio assets: `usdc`, `ethereum`, supplied as concrete quantities;
- quote currencies: `USD`, `MXN`, `USDC`, `BTC` (subject to Price Indexer
  historical resolution);
- one operator-configured 2026 demo window;
- Aave realized supply yield from DIS (via SPEC-001);
- historical valuation from the internal Price Indexer point-in-time
  `/prices/at` capability.

Out of scope:

- arbitrary assets, networks, or markets;
- arbitrary date ranges outside the configured demo window;
- borrowing, rebalancing, or deposits/withdrawals after the initial date;
- gas, taxes, fees, incentives, or reward tokens;
- APY display;
- financial advice;
- any public point-in-time, `asOf`, or historical price endpoint;
- production-grade datetime-to-block resolution.

## Public request

```json
{
  "allocations": [
    { "asset_slug": "usdc", "amount_raw": "100000000" },
    { "asset_slug": "ethereum", "amount_raw": "30000000000000000" }
  ],
  "quote_currency": "USD",
  "from_datetime": "2026-01-01T00:00:00Z",
  "to_datetime": "2026-06-01T00:00:00Z"
}
```

The caller provides concrete on-chain holdings, not capital weights. Mother API
computes the initial and estimated final portfolio value.

## Request rules

- `allocations` must be a non-empty list of supported assets (`usdc`,
  `ethereum`). Each entry is a concrete on-chain quantity, not a weight.
- Each allocation must include `asset_slug` and `amount_raw`.
- `amount_raw` must be a positive integer string in the asset's base units.
- Callers do not send `decimals`. Mother API owns decimals through the supported
  asset mapping (`usdc` = 6, `ethereum` = 18).
- An asset slug may appear at most once; duplicates are rejected.
- `quote_currency` must be one of `USD`, `MXN`, `USDC`, `BTC`, and the internal
  Price Indexer must be able to resolve the required historical quote prices.
- `from_datetime` and `to_datetime` must be UTC RFC3339 datetimes with
  `from_datetime < to_datetime`.
- `from_datetime` and `to_datetime` must match the configured demo window
  exactly (see Demo block resolution). The demo supports only that one window.
- Empty allocations, unsupported assets, and unsupported windows are rejected
  before any dependency call.

## Public asset mapping

| Public slug | Meaning                | Decimals | DIS asset | Price Indexer slug |
| ----------- | ---------------------- | -------- | --------- | ------------------ |
| `usdc`      | USD Coin               | `6`      | `USDC`    | `usdc`             |
| `ethereum`  | ETH/WETH Aave exposure | `18`     | `WETH`    | `ethereum`         |

Mother API owns this public-to-internal mapping, including decimals, for this
endpoint only.

## Demo block resolution

Phase 1 does not implement generic datetime-to-block resolution. The demo
exposes exactly **one** fixed window, configured by the operator as a single
pair of datetimes mapped to a single pair of Ethereum block numbers.

```text
(DEMO_FROM_DATETIME, DEMO_TO_DATETIME)
        -> (DEMO_FROM_BLOCK, DEMO_TO_BLOCK)
```

Rules:

- A request whose `from_datetime`/`to_datetime` do not equal the configured demo
  window is rejected with `unsupported_demo_window`.
- The resolved block range is the configured `from_block`/`to_block`. There is
  no interpolation and no per-request block estimation.
- Block resolution is demo-grade and must be disclosed in the response via the
  `demo_estimated_block_resolution` warning.

Mother API must not call Bigwig, call external block explorers at request time,
perform archive RPC calls, or present the datetime-to-block mapping as exact.

## Internal dependencies

### DIS (via SPEC-001)

Mother API calls the SPEC-001 client for each supplied Aave asset, sending the
resolved demo block range:

```json
{
  "chain_id": 1,
  "market": "aave-v3-ethereum",
  "asset": "USDC",
  "from_block": 19800000,
  "to_block": 19900000,
  "include_annualized_apy_estimate": false
}
```

- Mother API uses `realized_yield` only and never exposes APY.
- Request construction, retries, decimal handling, and DIS error mapping are
  owned by SPEC-001 and are not re-specified here.
- DIS warnings relevant to an asset are preserved in that asset's response
  block.

### Price Indexer (internal point-in-time pricing)

Mother API resolves historical valuation through the internal Price Indexer
point-in-time `/prices/at` capability, quoting in the requested
`quote_currency`:

```text
price_indexer.get_price_at("ethereum", quote_currency, from_datetime)
price_indexer.get_price_at("ethereum", quote_currency, to_datetime)
price_indexer.get_price_at("usdc",     quote_currency, from_datetime)
price_indexer.get_price_at("usdc",     quote_currency, to_datetime)
```

- ETH valuation always requires point-in-time prices at both window boundaries.
- USDC valuation:
  - when `quote_currency` is `USD` or `USDC`, Mother API may apply the
    stablecoin valuation policy (treat 1 USDC ≈ 1 quote unit) and skip the
    price lookup;
  - when `quote_currency` is `MXN` or `BTC`, Mother API must use Price Indexer
    historical conversion at both boundaries.
- This capability is internal to the portfolio estimator. It is not exposed
  publicly and adds no public point-in-time, `asOf`, or historical price route.
- Phase 1 does not model USDC depeg risk.

## Estimation model

For each supplied asset, Mother API derives units from the provided quantity:

```text
initial_units_raw         = amount_raw
estimated_final_units_raw = floor_to_base_unit(amount_raw * (1 + realized_yield))
```

`realized_yield` is the Aave V3 realized supply yield for that asset over the
resolved demo block range (DIS, via SPEC-001).
`estimated_final_units_raw` is an integer string. It is computed with
decimal-safe math and rounded down to the asset base unit after applying
`realized_yield`.

Valuation, in the requested `quote_currency`:

For `usdc`:

```text
initial_value = units_to_quote(initial_units_raw, usdc, from_datetime)
final_value   = units_to_quote(estimated_final_units_raw, usdc, to_datetime)
```

where `units_to_quote` applies the stablecoin policy for `USD`/`USDC` and Price
Indexer point-in-time conversion for `MXN`/`BTC`.

For `ethereum` (public `ethereum` is internal WETH / Aave ETH exposure):

```text
initial_value = (initial_units_raw / 10^18) * eth_price_at_from_datetime
final_value   = (estimated_final_units_raw / 10^18) * eth_price_at_to_datetime
```

Portfolio totals:

```text
initial_portfolio_value         = sum(asset.initial_value)
estimated_final_portfolio_value = sum(asset.estimated_final_value)
estimated_gain                  = estimated_final_portfolio_value - initial_portfolio_value
estimated_gain_percent          = estimated_gain / initial_portfolio_value
```

All math must use decimal-safe types. The implementation must not use `f32`,
`f64`, or any lossy floating-point conversion for money, token units, prices, or
yields.

## Public response

```json
{
  "ok": true,
  "protocol": "aave-v3",
  "network_slug": "eth-mainnet",
  "market": "aave-v3-ethereum",
  "quote_currency": "USD",
  "from_datetime": "2026-01-01T00:00:00Z",
  "to_datetime": "2026-06-01T00:00:00Z",
  "initial_portfolio_value": { "amount": "118.000000", "currency": "USD" },
  "estimated_final_portfolio_value": { "amount": "126.420000", "currency": "USD" },
  "estimated_gain": { "amount": "8.420000", "currency": "USD" },
  "estimated_gain_percent": "0.07135593",
  "assets": [
    {
      "asset_slug": "usdc",
      "amount_raw": "100000000",
      "decimals": 6,
      "initial_units_raw": "100000000",
      "estimated_final_units_raw": "101500000",
      "initial_value": { "amount": "100.000000", "currency": "USD" },
      "estimated_final_value": { "amount": "101.500000", "currency": "USD" },
      "estimated_gain": { "amount": "1.500000", "currency": "USD" },
      "realized_yield": "0.015",
      "price_from": null,
      "price_to": null,
      "yield_source": "iron-burrow-defi-intelligence-service",
      "price_source": null,
      "warnings": []
    },
    {
      "asset_slug": "ethereum",
      "amount_raw": "30000000000000000",
      "decimals": 18,
      "initial_units_raw": "30000000000000000",
      "estimated_final_units_raw": "30090000000000000",
      "initial_value": { "amount": "18.000000", "currency": "USD" },
      "estimated_final_value": { "amount": "24.920000", "currency": "USD" },
      "estimated_gain": { "amount": "6.920000", "currency": "USD" },
      "realized_yield": "0.003",
      "price_from": { "amount": "600.000000", "currency": "USD" },
      "price_to": { "amount": "828.182120", "currency": "USD" },
      "yield_source": "iron-burrow-defi-intelligence-service",
      "price_source": "iron-burrow-price-indexer",
      "warnings": []
    }
  ],
  "block_resolution": {
    "mode": "demo_fixed_window",
    "precision": "estimated",
    "from_block": 19800000,
    "to_block": 19900000
  },
  "verification": "estimated_from_onchain_yield_and_historical_prices",
  "confidence": "degraded",
  "warnings": ["demo_estimated_block_resolution"]
}
```

## Response rules

- Monetary values use a `{ "amount": <string>, "currency": <string> }` object
  denominated in the requested `quote_currency`.
- `amount`, all `*_raw` fields, `realized_yield`, and `price_*` values are JSON
  strings; clients must not parse them as floats.
- `price_from` and `price_to` represent the price per one human unit of the
  asset in the requested `quote_currency`.
- `price_from` and `price_to` are `null` for an asset when no price lookup was
  performed (USDC under the `USD`/`USDC` stablecoin policy).
- The response must not include annualized APY.
- The top-level `confidence` is `degraded` if any asset is degraded or if the
  demo-grade block-resolution warning applies to the request.
- DIS and Price Indexer warnings are preserved where relevant; unknown warnings
  do not fail the response. Asset-level warnings are reserved for dependency
  warnings that apply to that asset.
- The `demo_estimated_block_resolution` warning is top-level only because the
  fixed demo block range affects the whole request.
- The response is an estimate from Aave realized yield plus historical prices.
  It is not financial advice and not a production-grade simulator.

## Error behavior

| Condition                               | Public error code           |
| --------------------------------------- | --------------------------- |
| Unsupported asset                       | `unsupported_asset`         |
| Empty or duplicate allocations          | `invalid_allocation`        |
| Invalid `amount_raw`                    | `invalid_amount`            |
| Unsupported quote currency              | `unsupported_quote_currency`|
| Invalid date range                      | `invalid_date_range`        |
| Datetimes not equal to demo window      | `unsupported_demo_window`   |
| Demo window not configured              | `portfolio_demo_unavailable`|
| DIS unavailable                         | `dependency_unavailable`    |
| Price Indexer unavailable               | `dependency_unavailable`    |
| Historical price unavailable            | `price_unavailable`         |
| Unexpected dependency response          | `dependency_contract_error` |
| Unexpected internal error               | `internal_error`            |

Dependency internals, exception messages, and raw upstream error bodies must
not be leaked to public callers.

## Configuration

This spec reuses the SPEC-001 DIS client configuration (`DIS_BASE_URL`,
`DIS_REQUEST_TIMEOUT_MS`, `DIS_RETRY_MAX_ATTEMPTS`) and the existing Price
Indexer client configuration (`PRICE_INDEXER_URL`, `PRICE_QL_INTERNAL_TOKEN`,
`PRICE_INDEXER_TIMEOUT_MS`). It adds the demo window, following the existing
optional-config + graceful-degradation pattern in
[src/config.rs](../../src/config.rs).

| Variable                          | Default | Description                                              |
| --------------------------------- | ------- | -------------------------------------------------------- |
| `PORTFOLIO_AAVE_DEMO_FROM_DATETIME` | unset | Demo window start, UTC RFC3339.                          |
| `PORTFOLIO_AAVE_DEMO_TO_DATETIME`   | unset | Demo window end, UTC RFC3339.                            |
| `PORTFOLIO_AAVE_DEMO_FROM_BLOCK`    | unset | Ethereum block for the demo window start.                |
| `PORTFOLIO_AAVE_DEMO_TO_BLOCK`      | unset | Ethereum block for the demo window end.                  |

Behavior:

- If any demo variable is unset or invalid, the endpoint is disabled and
  returns `portfolio_demo_unavailable`. The service still starts normally.
- If `DIS_BASE_URL` is unset, the endpoint returns `dependency_unavailable`.
- If the Price Indexer client is disabled and a historical price lookup is
  required (any `ethereum` allocation, or a `MXN`/`BTC` quote currency), the
  endpoint returns `dependency_unavailable`.

## Tests

The implementation should cover:

1. valid `usdc` + `ethereum` quantity request over the demo window in `USD`;
2. valid `usdc`-only request;
3. valid `ethereum`-only request;
4. rejection of unsupported asset slugs;
5. rejection of empty allocations and duplicate `asset_slug` entries;
6. rejection of non-positive or non-integer `amount_raw`;
7. rejection of unsupported `quote_currency`;
8. rejection of datetimes that do not equal the configured demo window;
9. `portfolio_demo_unavailable` when the demo window is not configured;
10. DIS request construction with the resolved demo block range;
11. ETH point-in-time `/prices/at` price lookups at both window boundaries;
12. USDC stablecoin policy for `USD`/`USDC` and Price Indexer conversion for
    `MXN`/`BTC`;
13. Mother-computed initial and estimated final portfolio value;
14. decimal-safe preservation of amount, units, price, yield, and value math;
15. public response does not expose APY;
16. dependency error mapping.

## Acceptance criteria

This spec is satisfied when Mother API has:

- `POST /v1/portfolio/aave-v3/estimate` accepting `allocations[]` as concrete
  asset quantities, with no public `initial_amount` and no public `weight_bps`;
- public support for `usdc` and `ethereum` only, with Mother-owned decimals;
- `quote_currency` support for `USD`, `MXN`, `USDC`, and `BTC`;
- a single configured 2026 demo window to block-range mapping;
- DIS calls via the SPEC-001 client using the resolved block range;
- internal Price Indexer point-in-time `/prices/at` lookups for historical
  valuation;
- Mother-computed initial and estimated final portfolio value;
- ETH valuation reflecting both Aave realized yield and historical ETH price
  movement;
- USDC valuation reflecting Aave realized yield and the quote conversion policy;
- decimal-safe portfolio math;
- a clear `demo_estimated_block_resolution` warning;
- no public APY fields;
- no public point-in-time, `asOf`, or historical price surface;
- no Bigwig or runtime block-explorer calls from Mother API;
- no generalized portfolio engine;
- `CONTRACTS.md` documenting the simplified public request and response.

## Rollout plan

- **Phase A** — review and accept this hackathon spec.
- **Phase B** — implement validation, asset allowlist, and demo window config.
- **Phase C** — wire DIS realized-yield calls (SPEC-001) for `usdc` and
  `ethereum`.
- **Phase D** — wire the existing internal Price Indexer point-in-time
  `/prices/at` capability for ETH and quote valuation, adding the Mother API
  client helper as needed.
- **Phase E** — add a smoke test for the ETH Mexico demo request.
- **Phase F** — update `CONTRACTS.md` for the new public endpoint.

## Contracts

This is a spec-only change. The public `CONTRACTS.md` entry for
`POST /v1/portfolio/aave-v3/estimate` (the simplified quantity-based request,
`quote_currency`, the response fields, and error codes) must be added in the
implementation change, per
[DOCS.md](../../DOCS.md) and [AGENTS.md](../../AGENTS.md), not in this spec.

## Sunset policy

This spec defines the hackathon profile only. After the hackathon, if portfolio
estimation moves to production-grade datetime resolution, this spec should be
marked `superseded` and replaced by a new production spec. The expected
production direction is:

```text
Mother API -> DIS with datetimes
DIS        -> Bigwig internally for block resolution and on-chain reads
Mother API -> Price Indexer for valuation
```

Until then, this spec remains intentionally narrow and demo-scoped.
