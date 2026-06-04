# Iron Burrow Mother API RS

Fresh Rust implementation of the Iron Burrow Mother API using Axum.

This repository is not a line-by-line port of the old TypeScript gateway. The TypeScript implementation remains in `_reference_implementation/` as temporary reference material while the Rust service takes over the minimal Production Alpha 1 contract.

## Reference Findings

The old TypeScript gateway was built as a broad API gateway. It includes health and status endpoints, public explorer routes, price routes, account/tracking routes, admin preview routes, API-key context middleware, rate limiting, response caching, database checks, and price-indexer checks.

Deployment-wise, the old service used the stable container name `iron-burrow-mother-api`, listened on port `3000`, and was expected to be reached by Caddy over a Docker network. The Rust service takes over those canonical deployment names after the old TypeScript app is stopped and discarded, but drops the gateway sprawl.

## Not Ported

- Event or holder indexing
- Full auth enforcement, balance debit, payment rails, or x402 boundaries
- Admin, explorer, account, and tracked-token routes
- TypeScript package/module architecture

## Endpoint Contract

`GET /health`

```json
{
  "ok": true,
  "service": "iron-burrow-mother-api",
  "mascot": "Capitan Sousa",
  "message": "Happy squirrel, systems nominal."
}
```

`/health` is dependency-free.

`GET /v1/assets?limit=<limit>`

Lists active Mother API-owned global assets. `limit` is optional, defaults to
`100`, and is clamped to `1000`. List responses include USD price enrichment
from the internal price-indexer Query Layer when configured; otherwise each
asset returns a stable unavailable price object.

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

`GET /v1/assets/{slug}`

Returns one active asset plus the network-specific chain maps the UI can use to
render asset detail pages. Asset detail always includes a stable `price` object.
If the price-indexer Query Layer is not configured, unavailable, or has no price
for the slug, the asset response still succeeds with `price.status` set to
`"unavailable"`.

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
    },
    {
      "network": {
        "slug": "arbitrum-one",
        "name": "Arbitrum One",
        "caip2": "eip155:42161"
      },
      "is_native": false,
      "address": "0xaf88d065e77c8cc2239327c5edb3a432268e5831"
    },
    {
      "network": {
        "slug": "base",
        "name": "Base",
        "caip2": "eip155:8453"
      },
      "is_native": false,
      "address": "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913"
    },
    {
      "network": {
        "slug": "near",
        "name": "NEAR Mainnet",
        "caip2": "near:mainnet"
      },
      "is_native": false,
      "address": "17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1"
    },
    {
      "network": {
        "slug": "mantle",
        "name": "Mantle",
        "caip2": "eip155:5000"
      },
      "is_native": false,
      "address": "0x09bc4e0d864854c6afb6eb9a9cdf58ac190d0df9"
    }
  ]
}
```

### Deterministic Price Evidence

Mother API exposes a small deterministic price evidence surface backed by the
internal price-indexer Query Layer. Mother API does not read price-indexer
database tables directly. These endpoints return structured evidence only; they
do not predict prices, call an LLM, or return investment advice.

`GET /v1/assets/{slug}/price/latest`

Returns the latest known USD price for a canonical Mother API asset. Unknown
asset slugs use the same `404 asset_not_found` behavior as `GET
/v1/assets/{slug}`. If the price-indexer Query Layer is unavailable, the route
returns `503 price_indexer_unavailable`.

```json
{
  "ok": true,
  "type": "asset_price_latest",
  "asset": {
    "slug": "ethereum",
    "symbol": "ETH"
  },
  "price": {
    "currency": "USD",
    "value": "3811.450000",
    "published_at": "2026-05-29T00:00:00Z",
    "source": "chainlink"
  },
  "billing": {
    "billable": true,
    "currency": "USD",
    "amount": "0.000100"
  }
}
```

`GET /v1/assets/{slug}/signal/price-stats?window=7d`

`GET /v1/assets/{slug}/signal/price-stats?fromDate=2020-05-21&toDate=2020-05-29`

Returns deterministic statistics for normalized hourly USD price points. Recipe
name: `price_stats_v1`.

Supported `window` values are `7d`, `1w`, and `1m`. `7d` and `1w` are 7 days;
`1m` is 31 days for alpha. Explicit date mode requires both `fromDate` and
`toDate` in strict `YYYY-MM-DD` format. `window` and date mode are mutually
exclusive. Future dates, reversed date ranges, duplicate/unknown query
parameters, empty values, and ranges longer than 31 days return `400
invalid_price_signal_query`.

```json
{
  "ok": true,
  "type": "price_stats_signal",
  "asset": {
    "slug": "ethereum",
    "symbol": "ETH"
  },
  "signal": {
    "type": "price_stats",
    "recipe": "price_stats_v1",
    "status": "found",
    "range": {
      "mode": "window",
      "window": "7d",
      "from": "2026-05-22T00:00:00Z",
      "to": "2026-05-29T00:00:00Z"
    },
    "input": {
      "currency": "USD",
      "granularity": "1h",
      "observations": 168,
      "source_service": "price-indexer"
    },
    "stats": {
      "first_price": "3720.120000",
      "last_price": "3811.450000",
      "min_price": "3602.100000",
      "max_price": "3890.440000",
      "avg_price": "3744.910000",
      "change_abs": "91.330000",
      "change_pct": "2.454920",
      "observations": 168
    },
    "billing": {
      "billable": true,
      "currency": "USD",
      "amount": "0.000500"
    },
    "source": {
      "service": "price-indexer",
      "freshness": "historical"
    }
  }
}
```

`GET /v1/assets/{slug}/signal/price-trend?window=7d`

`GET /v1/assets/{slug}/signal/price-trend?fromDate=2020-05-21&toDate=2020-05-29`

Returns deterministic trend evidence using ordinary least squares. Recipe name:
`price_trend_evidence_v1`. The included models are `linear_raw_price`,
`log_linear_price`, and `indexed_linear_price`. The log model is skipped when
any price is non-positive; the indexed model is skipped when the first price is
non-positive. Agreement values are `positive`, `negative`, `mixed`, `flat`, or
`insufficient_data`.

```json
{
  "ok": true,
  "type": "price_trend_signal",
  "asset": {
    "slug": "ethereum",
    "symbol": "ETH"
  },
  "signal": {
    "type": "price_trend_evidence",
    "recipe": "price_trend_evidence_v1",
    "status": "found",
    "models": [
      {
        "name": "linear_raw_price",
        "transform": "price",
        "status": "included",
        "direction": "positive",
        "slope_per_day": "13.047143",
        "r_squared": "0.420100"
      }
    ],
    "evidence": {
      "positive_models": 3,
      "negative_models": 0,
      "flat_models": 0,
      "skipped_models": 0,
      "total_models": 3,
      "agreement": "positive"
    },
    "billing": {
      "billable": true,
      "currency": "USD",
      "amount": "0.001000"
    },
    "source": {
      "service": "price-indexer",
      "freshness": "historical"
    }
  }
}
```

Insufficient data is a stable non-billable response with `status:
"insufficient_data"`, `stats: null`, an empty model list for trend, and
`billing.billable: false`.

`GET /v1/resolve?q=<query>`

Resolves broad Sentinel search queries against Mother API-owned global assets.
Unknown searches return a successful unresolved response with recommendations
instead of forcing the frontend into a blind 404.

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

Invalid query responses are stable:

- invalid `limit`: `400 invalid_limit`
- missing or empty `q`: `400 missing_query`
- trimmed `q` over 128 characters: `400 query_too_long`
- invalid price signal query: `400 invalid_price_signal_query`
- configured database unavailable: `503 database_unavailable`
- configured price-indexer unavailable for price evidence: `503 price_indexer_unavailable`

## Metering Alpha

This release adds a quote-only metering foundation. API keys are access
credentials; accounts own balances and plans; usage belongs in a ledger. The
current endpoints include billing quote metadata but do not require API keys,
debit balances, or write usage ledger rows.

Supported real units for alpha are:

- `USD_MICRO`: 1 USD = 1,000,000 USD_MICRO
- `BTC_SATS`: 1 BTC = 100,000,000 sats

Public USD amounts are rendered as six-decimal strings. BTC support is modeled
as integer sats in the quote layer.

Conceptual API key types are:

- `DEMO_LIKE`: revocable demo/hackathon access, no balance debit, future strict
  rate limits.
- `ONE_TIME_API`: prepaid account-owned access using account balance.
- `SHREW_SUBSCRIPTION`: account/plan-owned access for the Musarana etrusca
  tier.

Alpha USD prices:

| Operation | Range | Amount |
| --- | --- | --- |
| `price.latest` | none | `0.000100` |
| `signal.price_stats` | <= 7 days | `0.000500` |
| `signal.price_stats` | <= 31 days | `0.001500` |
| `signal.price_trend` | <= 7 days | `0.001000` |
| `signal.price_trend` | <= 31 days | `0.003000` |

Validation errors, auth errors, rate-limit errors, upstream unavailable, and
insufficient data are not billable. Successful `found` responses are billable.

Current non-goals: no payment platform, no Stripe, no Coinbase, no x402, no BTC
payment rails, no LLM interpretation, no ML, and no investment advice.

## Configuration

| Variable | Default | Description |
| --- | --- | --- |
| `APP_ENV` | `development` | Runtime environment label. |
| `HTTP_HOST` | `0.0.0.0` | Bind host. |
| `HTTP_PORT` | `3000` | Bind port. |
| `DATABASE_URL` | unset | Optional Postgres URL for `mother_api.global_asset` resolver reads. |
| `PRICE_INDEXER_URL` | unset | Optional price-indexer Query Layer base URL, for example `http://price-indexer:3010`. Signal endpoints call `/internal/v1/prices/*` below this base URL. |
| `PRICE_QL_INTERNAL_TOKEN` | unset | Optional internal bearer token for price-indexer Query Layer calls. |
| `PRICE_INDEXER_TIMEOUT_MS` | `2000` | Optional timeout for price-indexer Query Layer calls. |
| `RUST_LOG` | `iron_burrow_mother_api_rs=info,tower_http=info` | Optional tracing filter. |

Price enrichment is enabled only when both `PRICE_INDEXER_URL` and
`PRICE_QL_INTERNAL_TOKEN` are set. Missing or failing price configuration does
not fail startup and does not fail asset detail pages; Mother API returns a
stable unavailable price state instead. Dedicated price evidence endpoints
return `503 price_indexer_unavailable` when the Query Layer is not configured or
unavailable.

## Database

Mother API owns a minimal global asset catalog for product-facing asset search
and routing:

- `mother_api.global_asset`: chain-agnostic assets such as Bitcoin, ETH, USDC,
  WBTC, Mantle, NEAR, and Gold.
- `mother_api.network`: networks such as Bitcoin mainnet, Ethereum mainnet,
  Base, and Mantle.
- `mother_api.asset_chain_map`: native assets and deployed token
  representations on each network.
- `mother_api.accounts`: future metering account owners.
- `mother_api.api_keys`: future access credentials for demo-like, one-time, and
  subscription usage.
- `mother_api.account_balances`: future integer minor-unit balances.
- `mother_api.usage_price_catalog`: alpha operation prices in USD_MICRO and
  BTC_SATS.
- `mother_api.usage_ledger`: future usage records; this alpha slice quotes
  usage but does not write ledger rows.

Price-indexer, chain indexer, and infra-gateway tables remain out of scope for
this service. Mother API consumes price-indexer through its Query Layer and does
not read price-indexer database tables directly.

Run migrations with `sqlx-cli` when `DATABASE_URL` points at the target database:

```sh
sqlx migrate run
```

Docker Compose runs the same command through the `db-migrate` service. Local
Compose keeps the convenience behavior of migrating before starting the API.
Production deploys should run `db-migrate` explicitly before restarting the API.

The seed catalog is production-alpha data, even though the current migration
filename still says `demo`. It includes AAVE, AUSD, BTC, USDS, ETH, FBTC, GHO,
MNT, MPDAO, NEAR, STNEAR, USDC, USDT, USDT0, USDe, WBTC, WETH, cmETH, mETH,
sUSDe, and Gold as assets. Bitcoin mainnet, Ethereum mainnet, Base, Arbitrum
One, Mantle, and NEAR are seeded as networks.

## Local Run

```sh
cargo run
```

```sh
curl -i http://localhost:3000/health
curl -i 'http://localhost:3000/v1/assets?limit=20'
curl -i 'http://localhost:3000/v1/assets/ethereum/price/latest'
curl -i 'http://localhost:3000/v1/assets/ethereum/signal/price-stats?window=7d'
curl -i 'http://localhost:3000/v1/assets/ethereum/signal/price-trend?window=7d'
curl -i 'http://localhost:3000/v1/assets/ethereum/signal/price-stats?fromDate=2020-05-21&toDate=2020-05-29'
curl -i 'http://localhost:3000/v1/resolve?q=usdc%20coin%20usd'
curl -i 'http://localhost:3000/v1/resolve?q=oro%20de%20ley'
curl -i 'http://localhost:3000/v1/resolve?q=some%20unknown%20thing'
```

With Docker:

```sh
cp .env.example .env
docker compose up --build
```

To run the local migration service directly:

```sh
docker compose run --rm db-migrate
```

## Publishing

Pushing an immutable release tag publishes one production image to GHCR:

```sh
git tag v0.1.2
git push origin v0.1.2
```

The workflow publishes only:

```text
ghcr.io/iron-burrow/iron-burrow-mother-api-rs:v0.1.2
```

It does not publish `latest`. The same image contains the API binary and
`sqlx-cli`; the `db-migrate` service runs:

```sh
sqlx migrate run
```

## Production Deploy

Production uses two external Docker networks:

- `iron-burrow-public-net`: shared only by Caddy and `iron-burrow-mother-api`.
- `iron-burrow-net`: shared only by Postgres, migrations, and `iron-burrow-mother-api`.

Caddy is the only public entrypoint and publishes ports `80` and `443`. The API
joins both networks, exposes container port `3000` without publishing it to the
host, and is reached by Caddy as `mother-api:3000` on `iron-burrow-public-net`.
Postgres and `db-migrate` stay on `iron-burrow-net`.

The price-indexer service should also join `iron-burrow-net` when it runs from a
separate Compose project. Mother API can then reach it by Docker DNS, commonly
`http://price-indexer:3010`, and the price-indexer does not need to publish port
`3010` to the host.

```sh
docker network create iron-burrow-net
docker network create iron-burrow-public-net
```

If the network already exists, Docker will report that and no action is needed.

```sh
cp .env.production.example .env.production
# Edit .env.production with production values.
```

The initial production-alpha deploy uses the pinned image tag `v0.1.0`:

```sh
IRON_BURROW_MOTHER_API_TAG=v0.1.0
```

Do not deploy production from `latest`; keep deploys tied to explicit release
tags so rollback and audit stay boring.

Pull the immutable image tag, run migrations explicitly, then start or restart
the API:

```sh
docker compose --env-file .env.production -f compose.yaml -f compose.prod.yaml pull iron-burrow-mother-api db-migrate
docker compose --env-file .env.production -f compose.yaml -f compose.prod.yaml run --rm db-migrate
docker compose --env-file .env.production -f compose.yaml -f compose.prod.yaml up -d iron-burrow-mother-api
docker compose --env-file .env.production -f compose.yaml -f compose.prod.yaml ps
```

Confirm both services resolve to the same image name and tag:

```sh
docker compose --env-file .env.production -f compose.yaml -f compose.prod.yaml config
docker image ls ghcr.io/iron-burrow/iron-burrow-mother-api-rs
```

If migration fails, do not start the new API image. Keep or restore the previous
`IRON_BURROW_MOTHER_API_TAG` in `.env.production`, pull that tag if needed, and
start the API with the previous image:

```sh
docker compose --env-file .env.production -f compose.yaml -f compose.prod.yaml up -d iron-burrow-mother-api
```

Database rollback should be handled as a forward repair or backup restore unless
a specific migration has an explicitly tested down path.

Before assigning the canonical `iron-burrow-mother-api` container and
`mother-api` network alias to Rust, stop and remove the old TypeScript API. This
repo does not maintain a side-by-side old/new naming strategy.

Production verification:

```sh
curl -i https://api.ironburrow.com/health
curl -i https://api.ironburrow.com/v1/status
curl -i 'https://api.ironburrow.com/v1/assets?limit=1'
curl -i 'https://api.ironburrow.com/v1/resolve?q=usdc'
```

`IRON_BURROW_MOTHER_API_TAG` controls the shared production image used by both
`iron-burrow-mother-api` and `db-migrate`:
`ghcr.io/iron-burrow/iron-burrow-mother-api-rs`.

Before cutover on the VPS, verify that `api.ironburrow.com` points to the VPS,
that Caddy serves only the intended Rust routes (`/health` and `/v1/*`), and
that old TypeScript routes are gone or return `404`.

Render the effective production config before deploying:

```sh
docker compose --env-file .env.production -f compose.yaml -f compose.prod.yaml config
```

## Development Checks

```sh
cargo fmt --check
cargo check
cargo test
```
