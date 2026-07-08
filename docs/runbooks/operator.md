---
status: active
owner: iron-burrow
last_reviewed: 2026-07-02
agent_edit_policy: update_when_relevant
---

# Operator Runbook

Production operator commands for private Beta API-key management. Assumes the
operator is on the VPN, connected to the production host, and the Mother API
Docker Compose stack is running.

Run commands from the production repository root:

```bash
cd ~/apps/iron-burrow-mother-api-rs
```

## Confirm Runtime State

```bash
docker ps --format 'table {{.Names}}\t{{.Status}}'

docker compose \
  --env-file .env.production \
  -f compose.yaml \
  -f compose.prod.yaml \
  config --services
```

Expected services include:

```text
caddy
postgres
db-apply
iron-burrow-mother-api
```

Confirm the running API container has the private Beta surface and transfer
search enabled:

```bash
docker compose \
  --env-file .env.production \
  -f compose.yaml \
  -f compose.prod.yaml \
  exec -T iron-burrow-mother-api sh -lc \
  'printf "%s\n" \
    "PUBLIC_API_SURFACE=$PUBLIC_API_SURFACE" \
    "ERC20_TRANSFERS_ENABLED=$ERC20_TRANSFERS_ENABLED" \
    "BIGWIG_REQUEST_TIMEOUT_MS=$BIGWIG_REQUEST_TIMEOUT_MS" \
    "INFRA_GATEWAY_URL=$INFRA_GATEWAY_URL"'
```

Do not print `INFRA_GATEWAY_TOKEN`, raw API keys, key hashes, or full
`Authorization` headers.

## Issue First Internal API Key

Issue the first internal key for Iron Burrow testing from inside the running
Mother API container:

```bash
issue_output="$(docker compose \
  --env-file .env.production \
  -f compose.yaml \
  -f compose.prod.yaml \
  exec -T iron-burrow-mother-api \
  mother-api admin api-key issue \
    --consumer-slug iron-burrow-internal \
    --display-name "Iron Burrow Internal" \
    --category internal \
    --label "founder beta testing key" \
    --requests-per-minute 60 \
    --requests-per-day 5000 \
    --format json)"

printf '%s\n' "$issue_output" | jq '{ok, consumer_slug, key_prefix}'
export IB_API_KEY="$(printf '%s\n' "$issue_output" | jq -r '.api_key')"
export IB_API_KEY_PREFIX="$(printf '%s\n' "$issue_output" | jq -r '.key_prefix')"
```

The full API key is printed only by the issue command. Keep it in the operator
shell only for the active test session, or move it directly into the approved
secret store. Do not commit it, paste it into chat, save it in shell history,
include it in screenshots, or write it to logs.

Set the API URL and auth header for local checks:

```bash
export IB_API="${IB_API:-https://${CADDY_DOMAIN:-api.ironburrow.com}}"
export AUTH_HEADER="Authorization: Bearer $IB_API_KEY"
```

## Verify The Key Works

Check `/health` without credentials:

```bash
curl -sS "$IB_API/health" \
  | jq -e '.ok == true and .service == "iron-burrow-mother-api"'
```

Check that the key reaches protected-route validation. This intentionally sends
a reserved `chain` alias and should return `400 invalid_request`, proving auth
accepted the key before route validation rejected the body.

```bash
jq -n '{
  as_of: {kind: "latest"},
  account: {
    network_slug: "eth-mainnet",
    address: "0x1234567890abcdef1234567890abcdef1234beef",
    chain: "eth-mainnet"
  },
  quote_currency: "USD",
  tokens: {asset_slugs: ["ethereum"], contract_addresses: []}
}' > /tmp/mother-api-key-check.json

status="$(curl -sS -o /tmp/mother-api-key-check.out.json -w '%{http_code}' \
  -X POST "$IB_API/v1/balances" \
  -H 'Content-Type: application/json' \
  -H "$AUTH_HEADER" \
  -d @/tmp/mother-api-key-check.json)"
echo "POST /v1/balances -> HTTP $status"
test "$status" = "400"
jq -e '.ok == false and .error.code == "invalid_request"' \
  /tmp/mother-api-key-check.out.json
```

## List Keys And Usage

List issued keys for the internal consumer:

```bash
docker compose \
  --env-file .env.production \
  -f compose.yaml \
  -f compose.prod.yaml \
  exec -T iron-burrow-mother-api \
  mother-api admin api-key list \
    --consumer-slug iron-burrow-internal \
    --format json \
  | jq '.'
```

Inspect usage:

```bash
docker compose \
  --env-file .env.production \
  -f compose.yaml \
  -f compose.prod.yaml \
  exec -T iron-burrow-mother-api \
  mother-api admin api-key usage \
    --consumer-slug iron-burrow-internal \
    --days 7 \
    --format json \
  | jq '.'
```

List and usage output must not include raw API keys or key hashes.

## Revoke A Key

Revoke by key prefix only:

```bash
docker compose \
  --env-file .env.production \
  -f compose.yaml \
  -f compose.prod.yaml \
  exec -T iron-burrow-mother-api \
  mother-api admin api-key revoke \
    --key-prefix "$IB_API_KEY_PREFIX" \
    --format json \
  | jq '.'
```

Verify the revoked key no longer authenticates:

```bash
status="$(curl -sS -o /tmp/mother-api-key-revoked.out.json -w '%{http_code}' \
  -X POST "$IB_API/v1/balances" \
  -H 'Content-Type: application/json' \
  -H "$AUTH_HEADER" \
  -d @/tmp/mother-api-key-check.json)"
echo "revoked key -> HTTP $status"
test "$status" = "401"
jq -e '.ok == false and .error.code == "unauthorized"' \
  /tmp/mother-api-key-revoked.out.json
```

After verification, clear shell variables that contain secrets:

```bash
unset IB_API_KEY AUTH_HEADER issue_output
```
