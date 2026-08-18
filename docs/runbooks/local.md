---
status: active
owner: iron-burrow
last_reviewed: 2026-08-18
agent_edit_policy: update_when_relevant
---

# Run Mother API Locally (Fresh Environment)

This runbook is for local development on a brand new machine. Unlike
[deploy.md](deploy.md), this flow is not production-oriented and does not
require Bigwig connectivity.

Use this from the repository root.

## 1. Prerequisites

Install these tools first:

- Docker Desktop (or Docker Engine + Compose v2)
- Git
- `curl`
- `jq` (recommended for readable JSON output)

Clone the repository:

```bash
git clone <your-repo-url> iron-burrow-mother-api-rs
cd iron-burrow-mother-api-rs
```

## 2. Create local environment file

Create `.env` from the local template:

```bash
cp .env.example .env
```

Edit `.env` for a no-Bigwig local setup:

```bash
PUBLIC_API_SURFACE=alpha
ERC20_TRANSFERS_ENABLED=false
INFRA_GATEWAY_URL=
INFRA_GATEWAY_TOKEN=
```

Notes:

- In local mode without Bigwig, keep `ERC20_TRANSFERS_ENABLED=false` so
  `/v1/erc20-transfers/search` is not exposed.
- `PRICE_INDEXER_URL` can stay unset or blank locally.
- Keep `DATABASE_URL` pointing at Compose Postgres:
  `postgres://postgres:postgres@ibdb-postgres:5432/ibdb`.

## 3. Build and start local services

Start Postgres, apply migrations/reference data, and run Mother API:

```bash
docker compose up -d postgres db-apply iron-burrow-mother-api
```

Check container state:

```bash
docker ps --format 'table {{.Names}}\t{{.Status}}'
```

Expected running containers:

```text
ibdb-postgres
iron-burrow-mother-api
```

`db-apply` is a one-shot job and should exit successfully.

## 4. Verify API health

```bash
curl -sS http://localhost:3000/health | jq
```

Quick check for OpenAPI output:

```bash
curl -sS http://localhost:3000/openapi.json | jq '.info.title, .info.version'
```

## 5. Optional: run local Caddy hostnames

If you want `https://api.localhost` and `https://www.localhost` locally,
configure and start the Caddy profile:

```bash
export CADDY_API_DOMAIN=api.localhost
export CADDY_WEB_DOMAIN=www.localhost
docker compose --profile app-dev up -d caddy
```

Then test via Caddy:

```bash
curl -sS https://api.localhost/health | jq
```

## 6. Logs and troubleshooting

Tail API logs:

```bash
docker logs -f iron-burrow-mother-api
```

If startup fails, inspect the migration step:

```bash
docker logs iron-burrow-mother-api-db-apply
```

Re-run migration/reference apply manually:

```bash
docker compose run --rm db-apply
```

## 7. Stop and clean up

Stop services:

```bash
docker compose down
```

Stop and remove local Postgres data volume as well:

```bash
docker compose down -v
```

## 8. Optional: run from Rust directly (without Docker for API process)

You can run Postgres in Docker and execute the API from your host:

```bash
docker compose up -d postgres
DATABASE_URL=postgres://postgres:postgres@localhost:5432/ibdb cargo run -- db apply
DATABASE_URL=postgres://postgres:postgres@localhost:5432/ibdb cargo run
```

This path is useful for faster iteration with local Rust tooling, while still
using disposable local infrastructure.
