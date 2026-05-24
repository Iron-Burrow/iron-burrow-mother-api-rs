# Iron Burrow Mother API RS

Fresh Rust implementation of the Iron Burrow Mother API using Axum.

This repository is not a line-by-line port of the old TypeScript gateway. The TypeScript implementation remains in `_reference_implementation/` as temporary reference material while the Rust service takes over the minimal Production Alpha 1 contract.

## Reference Findings

The old TypeScript gateway was built as a broad API gateway. It includes health and status endpoints, public explorer routes, price routes, account/tracking routes, admin preview routes, API-key context middleware, rate limiting, response caching, database checks, and price-indexer checks.

Deployment-wise, the old service used the stable container name `iron-burrow-mother-api`, listened on port `3000`, and was expected to be reached by Caddy over a Docker network. The Rust service keeps those deployment assumptions but drops the gateway sprawl.

## Not Ported

- Price-indexer logic
- Event or holder indexing
- Database access and migrations
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

Both endpoints are dependency-free. They do not call Postgres, price-indexer, EVM indexer, Erigon, Chainlink, or any external service.

## Configuration

| Variable | Default | Description |
| --- | --- | --- |
| `APP_ENV` | `development` | Public environment value returned by `/v1/status`. |
| `HTTP_HOST` | `0.0.0.0` | Bind host. |
| `HTTP_PORT` | `3000` | Bind port. |
| `RUST_LOG` | `iron_burrow_mother_api_rs=info,tower_http=info` | Optional tracing filter. |

## Local Run

```sh
cargo run
```

```sh
curl -i http://localhost:3000/health
curl -i http://localhost:3000/v1/status
```

With Docker:

```sh
docker compose up --build
```

## Production Deploy

The production service attaches to an external Docker network named `iron-burrow-net` and exposes port `3000` only to that network. Caddy should reverse proxy to `iron-burrow-mother-api:3000`.

```sh
docker network create iron-burrow-net
```

If the network already exists, Docker will report that and no action is needed.

```sh
export IRON_BURROW_MOTHER_API_TAG=<release-tag>
docker compose -f compose.yaml -f compose.prod.yaml up -d
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
