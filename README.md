# Iron Burrow Mother API RS

Fresh Rust implementation of the Iron Burrow Mother API using Axum.

This repository is not a line-by-line port of the old TypeScript gateway. The TypeScript implementation remains in `_reference_implementation/` as temporary reference material while the Rust service takes over the minimal Production Alpha 1 contract.

## Reference Findings

The old TypeScript gateway was built as a broad API gateway. It includes health and status endpoints, public explorer routes, price routes, account/tracking routes, admin preview routes, API-key context middleware, rate limiting, response caching, database checks, and price-indexer checks.

Deployment-wise, the old service used the stable container name `iron-burrow-mother-api`, listened on port `3000`, and was expected to be reached by Caddy over a Docker network. The Rust service keeps those deployment assumptions but drops the gateway sprawl.

## Not Ported

- Price-indexer logic
- Event or holder indexing
- Auth, API keys, billing, or x402 boundaries
- Admin, explorer, account, tracked-token, and price routes
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

`GET /v1/status`

```json
{
  "ok": true,
  "service": "iron-burrow-mother-api",
  "version": "<crate version>",
  "environment": "<APP_ENV>",
  "mascot": "Capitan Sousa",
  "message": "Mother API is online.",
  "checks": {
    "app": "ok",
    "database": "skipped",
    "price_indexer": "not_connected",
    "evm_indexer": "not_connected"
  }
}
```

`/health` is dependency-free. `/v1/status` reports `checks.database` as `skipped`
when `DATABASE_URL` is not configured, otherwise it runs a lightweight `select 1`
and reports `reachable` or `unreachable`.

`GET /api/v1/resolve?q=<query>`

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

- missing or empty `q`: `400 missing_query`
- trimmed `q` over 128 characters: `400 query_too_long`
- configured database unavailable: `503 database_unavailable`

## Configuration

| Variable | Default | Description |
| --- | --- | --- |
| `APP_ENV` | `development` | Public environment value returned by `/v1/status`. |
| `HTTP_HOST` | `0.0.0.0` | Bind host. |
| `HTTP_PORT` | `3000` | Bind port. |
| `DATABASE_URL` | unset | Optional Postgres URL for `mother_api.global_asset` resolver reads. |
| `RUST_LOG` | `iron_burrow_mother_api_rs=info,tower_http=info` | Optional tracing filter. |

## Database

Mother API owns a minimal global asset catalog for product-facing asset search
and routing:

- `mother_api.global_asset`: chain-agnostic assets such as Bitcoin, ETH, USDC,
  WBTC, Mantle, NEAR, and Gold.
- `mother_api.network`: networks such as Bitcoin mainnet, Ethereum mainnet,
  Base, and Mantle.
- `mother_api.asset_chain_map`: native assets and deployed token
  representations on each network.

Price-indexer, chain indexer, and infra-gateway tables remain out of scope for
this service.

Run migrations with `sqlx-cli` when `DATABASE_URL` points at the target database:

```sh
sqlx migrate run
```

The demo seed includes Bitcoin, Ethereum, USDC, WBTC, Gold, Mantle, and NEAR as
assets. Bitcoin mainnet, Ethereum mainnet, Base, and Mantle are seeded as
networks.

## Local Run

```sh
cargo run
```

```sh
curl -i http://localhost:3000/health
curl -i http://localhost:3000/v1/status
curl -i 'http://localhost:3000/api/v1/resolve?q=usdc%20coin%20usd'
curl -i 'http://localhost:3000/api/v1/resolve?q=oro%20de%20ley'
curl -i 'http://localhost:3000/api/v1/resolve?q=some%20unknown%20thing'
```

With Docker:

```sh
cp .env.example .env
docker compose up --build
```

## Production Deploy

The production service attaches to an external Docker network named `iron-burrow-net` and exposes port `3000` only to that network. Caddy should reverse proxy to `iron-burrow-mother-api:3000`.

```sh
docker network create iron-burrow-net
```

If the network already exists, Docker will report that and no action is needed.

```sh
cp .env.example .env.production
# Edit .env.production with production values.
docker compose --env-file .env.production -f compose.yaml -f compose.prod.yaml up -d
```

Production verification:

```sh
curl -i https://api.ironburrow.com/health
curl -i https://api.ironburrow.com/v1/status
```

## Development Checks

```sh
cargo fmt --check
cargo check
cargo test
```
