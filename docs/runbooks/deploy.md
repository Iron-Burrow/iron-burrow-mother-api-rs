---
status: active
owner: iron-burrow
last_reviewed: 2026-07-08
agent_edit_policy: update_when_relevant
---

# Deploy Mother API Private Beta v0.3

Run this from the production host. This runbook deploys the Mother API service
only; production smoke checks live in [smoke-tests.md](smoke-tests.md).

## 1. Update the repository

```bash
cd ~/apps/iron-burrow-mother-api-rs
git status
```

If the working tree is clean, pull the release commit:

```bash
git pull
```

## 2. Update `.env.production`

```bash
vim .env.production
```

Confirm the production image tag, Beta surface, transfer route, and Bigwig
client settings:

```bash
IRON_BURROW_MOTHER_API_TAG=v0.3.x
PUBLIC_API_SURFACE=beta
INFRA_GATEWAY_URL=http://infra-gateway-hub:8080
INFRA_GATEWAY_TOKEN=<set-production-token>
BIGWIG_REQUEST_TIMEOUT_MS=30000
ERC20_TRANSFERS_ENABLED=true
```

A disabled ERC-20 transfer gate is a private Beta launch misconfiguration. The
first Beta release includes ERC-20 transfer search; if transfer smoke checks
fail, block the launch or roll back the Beta deployment rather than shipping a
reduced surface.

## 3. Render Compose

```bash
docker compose \
  --env-file .env.production \
  -f compose.yaml \
  -f compose.prod.yaml \
  config
```

Confirm the rendered API service includes:

```yaml
PUBLIC_API_SURFACE: beta
ERC20_TRANSFERS_ENABLED: "true"
INFRA_GATEWAY_URL: http://infra-gateway-hub:8080
BIGWIG_REQUEST_TIMEOUT_MS: "30000"
```

Confirm `INFRA_GATEWAY_TOKEN` is present in the rendered service environment,
but do not paste or store the token in logs, screenshots, or chat.

```bash
docker compose --env-file .env.production -f compose.yaml -f compose.prod.yaml config --services
```

Expected services:

```text
caddy
postgres
db-apply
iron-burrow-mother-api
```

## 4. Pull the release image

```bash
docker compose \
  --env-file .env.production \
  -f compose.yaml \
  -f compose.prod.yaml \
  pull
```

## 5. Start Postgres

```bash
docker compose \
  --env-file .env.production \
  -f compose.yaml \
  -f compose.prod.yaml \
  up -d postgres
```

Wait for `ibdb-postgres` to become healthy:

```bash
docker ps --format 'table {{.Names}}\t{{.Status}}'
docker logs --tail=100 ibdb-postgres
```

## 6. Apply database state

`db-apply` runs `mother-api db apply`, which applies embedded SQLx migrations
and then embedded reference data. Use the same image tag as the API service.

```bash
docker compose \
  --env-file .env.production \
  -f compose.yaml \
  -f compose.prod.yaml \
  run --rm db-apply
```

If this fails, do not roll out the API container.

## 7. Deploy the API

```bash
docker compose \
  --env-file .env.production \
  -f compose.yaml \
  -f compose.prod.yaml \
  up -d --no-deps --force-recreate iron-burrow-mother-api
```

Recreate Caddy only when the Caddyfile, domain, or network wiring changed:

```bash
docker compose \
  --env-file .env.production \
  -f compose.yaml \
  -f compose.prod.yaml \
  up -d --no-deps --force-recreate caddy
```

Otherwise, leave Caddy running; it proxies to the `mother-api:3000` service
alias on `iron-burrow-public-net`.

## 8. Run production smoke tests

Run the production smoke checks from [smoke-tests.md](smoke-tests.md). The
private Beta release is ready only when health, auth, balance, and ERC-20
transfer checks all pass.

If any ERC-20 transfer smoke check fails with `extraction_unavailable`,
`upstream_provider_timeout`, `extraction_timeout`, or a disabled route, treat
the deployment as not ready. Fix Bigwig or Mother API configuration and
redeploy, or roll back the Beta release.
