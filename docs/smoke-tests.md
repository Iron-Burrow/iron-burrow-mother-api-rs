---
status: active
owner: iron-burrow
last_reviewed: 2026-06-29
agent_edit_policy: update_when_relevant
---

# Production Smoke Tests

Brief runbook for the Beta balance surface from
[SPEC-008](specs/SPEC-008-balance-endpoint-beta-contract-hardening.md) and
the optional ERC-20 transfer search release gate from
[SPEC-007](specs/SPEC-007-public-erc-20-transfer-search-v1.md).

Run these from the production repository root. Requires `curl`, `jq`, `grep`,
`sed`, `awk`, and `seq`.

Mother API has no inbound authentication. Do not add bearer-token or API-key
headers to public smoke requests.

## Release Gate

Do not call the balance surface Beta-ready until:

- Mother API is deployed with `PUBLIC_API_SURFACE=beta`;
- health, single-balance, bulk-balance, validation-error, disabled-route, and
  unknown-route checks below pass;
- `cargo test` passes for the release commit.

Do not keep `ERC20_TRANSFERS_ENABLED=true` for first external users until:

- Bigwig Hub has `extraction.enabled: true`;
- Mother API has `INFRA_GATEWAY_URL`, `INFRA_GATEWAY_TOKEN`, and
  `BIGWIG_REQUEST_TIMEOUT_MS=30000`;
- transfer-search checks 1-9 below pass;
- transfer-search check 10 does not show an availability or timeout failure.

If any check fails after enabling the route, set `ERC20_TRANSFERS_ENABLED=false`
again in the target environment and redeploy before exposing the route.

## Setup

```bash
set +e; set +u; set +o pipefail 2>/dev/null || true

set -a
. ./.env.production
set +a

export IB_API="${IB_API:-https://${CADDY_DOMAIN:-api.ironburrow.com}}"
export JSON_HEADER='Content-Type: application/json'

export WATCHED_ADDRESS="${WATCHED_ADDRESS:-0xabc0000000000000000000000000000000000000}"
export BALANCE_ACCOUNT_A="${BALANCE_ACCOUNT_A:-0x1234567890abcdef1234567890abcdef1234beef}"
export BALANCE_ACCOUNT_B="${BALANCE_ACCOUNT_B:-0x2222222222222222222222222222222222222222}"
export USDC_CONTRACT='0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48'
export TEST_FROM_BLOCK="${TEST_FROM_BLOCK:-18600000}"
export TEST_TO_BLOCK="${TEST_TO_BLOCK:-18600500}"

echo "Testing Mother API at: $IB_API"
```

The watched address is intentionally synthetic. These smoke cases validate the
contract, filters, and failure behavior without depending on a funded wallet.

## Database Lifecycle Check

Use throwaway Docker containers to prove the production image can apply the full
database lifecycle from the Mother API binary, without `sqlx-cli` or `psql` in
the application image.

```bash
make smoke-db-lifecycle
```

The lifecycle smoke test builds the production image, runs
`mother-api db apply` twice against a clean disposable Postgres database,
verifies selected reference-data audit rows are unchanged on the second run,
starts `mother-api serve` from the same image, checks `/health`, and confirms
the application image does not contain `sqlx` or `psql`. The test harness may
use Postgres utility containers for readiness and database inspection.

## Test Payloads

```bash
jq -n \
  --arg address "$BALANCE_ACCOUNT_A" \
  '{
    as_of: {kind: "latest"},
    account: {
      network_slug: "eth-mainnet",
      address: $address,
      client_ref: "single-smoke"
    },
    quote_currency: "USD",
    assets: [
      {asset_slug: "ethereum"}
    ]
  }' > /tmp/mother-balance-single.json

jq -n \
  --arg address_a "$BALANCE_ACCOUNT_A" \
  --arg address_b "$BALANCE_ACCOUNT_B" \
  '{
    as_of: {kind: "latest"},
    accounts: [
      {
        network_slug: "base-mainnet",
        address: $address_a,
        client_ref: "base-smoke"
      },
      {
        network_slug: "eth-mainnet",
        address: $address_b,
        client_ref: "eth-smoke"
      }
    ],
    quote_currency: "USD",
    assets: [
      {asset_slug: "usdc"},
      {asset_slug: "ethereum"}
    ]
  }' > /tmp/mother-balance-bulk.json

jq -n \
  --arg address "$WATCHED_ADDRESS" \
  --argjson from "$TEST_FROM_BLOCK" \
  --argjson to "$TEST_TO_BLOCK" \
  '{
    network_slug: "eth-mainnet",
    address: $address,
    direction: "any",
    tokens: null,
    window: {from_block: $from, to_block: $to}
  }' > /tmp/mother-erc20-unfiltered.json

jq -n \
  --arg address "$WATCHED_ADDRESS" \
  --argjson from "$TEST_FROM_BLOCK" \
  --argjson to "$TEST_TO_BLOCK" \
  '{
    network_slug: "eth-mainnet",
    address: $address,
    direction: "any",
    tokens: {asset_slugs: ["usdc"], contract_addresses: []},
    window: {from_block: $from, to_block: $to}
  }' > /tmp/mother-erc20-usdc-slug.json

jq -n \
  --arg address "$WATCHED_ADDRESS" \
  --arg usdc "$USDC_CONTRACT" \
  --argjson from "$TEST_FROM_BLOCK" \
  --argjson to "$TEST_TO_BLOCK" \
  '{
    network_slug: "eth-mainnet",
    address: $address,
    direction: "any",
    tokens: {asset_slugs: [], contract_addresses: [$usdc]},
    window: {from_block: $from, to_block: $to}
  }' > /tmp/mother-erc20-usdc-contract.json

jq -n \
  --arg address "$WATCHED_ADDRESS" \
  --arg usdc "$USDC_CONTRACT" \
  --argjson from "$TEST_FROM_BLOCK" \
  --argjson to "$TEST_TO_BLOCK" \
  '{
    network_slug: "eth-mainnet",
    address: $address,
    direction: "any",
    tokens: {asset_slugs: ["usdc"], contract_addresses: [$usdc]},
    window: {from_block: $from, to_block: $to}
  }' > /tmp/mother-erc20-usdc-mixed.json
```

## Balance Beta Checks

### B1. Health And Beta Route Surface

Expected: health is up, known Alpha-only routes return
`403 endpoint_disabled`, and truly unknown routes return `404`.

```bash
curl -sS "$IB_API/health" \
  | jq -e '.ok == true and .service == "iron-burrow-mother-api"'

status="$(curl -sS -o /tmp/mother-disabled-route.out.json -w '%{http_code}' \
  "$IB_API/v1/status")"
echo "GET /v1/status -> HTTP $status"
test "$status" = "403"
jq -e '.ok == false and .error.code == "endpoint_disabled"' \
  /tmp/mother-disabled-route.out.json

status="$(curl -sS -o /tmp/mother-unknown-route.out -w '%{http_code}' \
  "$IB_API/v1/not-a-real-route")"
echo "GET /v1/not-a-real-route -> HTTP $status"
test "$status" = "404"
```

### B2. Single Balance

Expected: HTTP `200`, `ok: true`, single-balance response shape, canonical
`network_slug`, and either resolved positions or sanitized item-level errors.

```bash
status="$(curl -sS -o /tmp/mother-balance-single.out.json -w '%{http_code}' \
  -X POST "$IB_API/v1/balances" \
  -H "$JSON_HEADER" \
  -d @/tmp/mother-balance-single.json)"
echo "HTTP $status"
test "$status" = "200"
jq -e '.status as $status | .ok == true and .type == "balances" and (["complete", "partial", "failed"] | index($status) != null) and .account.network_slug == "eth-mainnet" and (.positions | type) == "array" and (.errors | type) == "array"' \
  /tmp/mother-balance-single.out.json
```

### B3. Bulk Balances

Expected: HTTP `200`, `ok: true`, bulk response shape, public summary counts,
and per-account result arrays.

```bash
status="$(curl -sS -o /tmp/mother-balance-bulk.out.json -w '%{http_code}' \
  -X POST "$IB_API/v1/balances/bulk" \
  -H "$JSON_HEADER" \
  -d @/tmp/mother-balance-bulk.json)"
echo "HTTP $status"
test "$status" = "200"
jq -e '.status as $status | .ok == true and .type == "balances_bulk" and (["complete", "partial", "failed"] | index($status) != null) and .summary.requested_accounts == 2 and .summary.requested_assets == 2 and .summary.requested_resolution_items == 4 and (.accounts | length) == 2' \
  /tmp/mother-balance-bulk.out.json
```

### B4. Balance Validation Errors

Expected: unsupported fields reject with `400 unknown_field`, and reserved
network aliases reject with `400 invalid_request`.

```bash
jq '.unexpected = true' \
  /tmp/mother-balance-single.json > /tmp/mother-balance-unknown-field.json

status="$(curl -sS -o /tmp/mother-balance-unknown-field.out.json -w '%{http_code}' \
  -X POST "$IB_API/v1/balances" \
  -H "$JSON_HEADER" \
  -d @/tmp/mother-balance-unknown-field.json)"
echo "unknown field -> HTTP $status"
test "$status" = "400"
jq -e '.ok == false and .error.code == "unknown_field"' \
  /tmp/mother-balance-unknown-field.out.json

jq '.account.chain = "eth-mainnet"' \
  /tmp/mother-balance-single.json > /tmp/mother-balance-reserved-alias.json

status="$(curl -sS -o /tmp/mother-balance-reserved-alias.out.json -w '%{http_code}' \
  -X POST "$IB_API/v1/balances" \
  -H "$JSON_HEADER" \
  -d @/tmp/mother-balance-reserved-alias.json)"
echo "reserved alias -> HTTP $status"
test "$status" = "400"
jq -e '.ok == false and .error.code == "invalid_request"' \
  /tmp/mother-balance-reserved-alias.out.json
```

## ERC-20 Transfer Search Checks

### 1. Health And Config

Expected: health is up and the transfer route is registered. `GET` should
return `405`; `404` means `ERC20_TRANSFERS_ENABLED` is still false.

```bash
curl -sS "$IB_API/health" \
  | jq -e '.ok == true and .service == "iron-burrow-mother-api"'

grep -E '^(PUBLIC_API_SURFACE|INFRA_GATEWAY_URL|INFRA_GATEWAY_TOKEN|BIGWIG_REQUEST_TIMEOUT_MS|BIGWIG_MAX_CONTRACT_ADDRESSES|ERC20_TRANSFERS_ENABLED|ERC20_TRANSFERS_MAX_TOKEN_FILTERS)=' .env.production \
  | sed 's/INFRA_GATEWAY_TOKEN=.*/INFRA_GATEWAY_TOKEN=<redacted>/'

curl -sS -o /dev/null -w 'GET /v1/erc20-transfers/search -> HTTP %{http_code}\n' \
  "$IB_API/v1/erc20-transfers/search"
```

### 2. Unfiltered Small Block Window

Expected: HTTP `200`, `ok: true`, and `limits.truncated` is a boolean.

```bash
status="$(curl -sS -o /tmp/mother-erc20-unfiltered.out.json -w '%{http_code}' \
  -X POST "$IB_API/v1/erc20-transfers/search" \
  -H "$JSON_HEADER" \
  -d @/tmp/mother-erc20-unfiltered.json)"
echo "HTTP $status"
jq -e '.ok == true and .type == "erc20_transfer_search" and .network_slug == "eth-mainnet" and (.transfers | type) == "array" and .limits.max_rows == 5000 and (.limits.truncated | type) == "boolean"' \
  /tmp/mother-erc20-unfiltered.out.json
```

### 3. USDC Asset Slug Search

Expected: USDC resolves to its Ethereum mainnet ERC-20 contract.

```bash
status="$(curl -sS -o /tmp/mother-erc20-usdc-slug.out.json -w '%{http_code}' \
  -X POST "$IB_API/v1/erc20-transfers/search" \
  -H "$JSON_HEADER" \
  -d @/tmp/mother-erc20-usdc-slug.json)"
echo "HTTP $status"
jq -e '.ok == true and .token_filters.requested.asset_slugs == ["usdc"] and any(.token_filters.resolved_contract_addresses[]; .asset_slug == "usdc" and .contract_address == "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48")' \
  /tmp/mother-erc20-usdc-slug.out.json
```

### 4. Explicit USDC Contract Search

Expected: the explicit contract is accepted and normalized.

```bash
status="$(curl -sS -o /tmp/mother-erc20-usdc-contract.out.json -w '%{http_code}' \
  -X POST "$IB_API/v1/erc20-transfers/search" \
  -H "$JSON_HEADER" \
  -d @/tmp/mother-erc20-usdc-contract.json)"
echo "HTTP $status"
jq -e '.ok == true and any(.token_filters.resolved_contract_addresses[]; .contract_address == "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48" and .source == "contract_address")' \
  /tmp/mother-erc20-usdc-contract.out.json
```

### 5. Mixed USDC And Explicit Contract Search

Expected: duplicate USDC filters dedupe to one concrete search contract.

```bash
status="$(curl -sS -o /tmp/mother-erc20-usdc-mixed.out.json -w '%{http_code}' \
  -X POST "$IB_API/v1/erc20-transfers/search" \
  -H "$JSON_HEADER" \
  -d @/tmp/mother-erc20-usdc-mixed.json)"
echo "HTTP $status"
jq -e '.ok == true and .token_filters.requested.asset_slugs == ["usdc"] and (.token_filters.resolved_contract_addresses | length) == 1 and .token_filters.resolved_contract_addresses[0].asset_slug == "usdc"' \
  /tmp/mother-erc20-usdc-mixed.out.json
```

### 6. Ethereum Native Asset Rejection

Expected: HTTP `422`, `asset_not_erc20_on_network`.

```bash
jq '.tokens = {asset_slugs: ["ethereum"], contract_addresses: []}' \
  /tmp/mother-erc20-unfiltered.json > /tmp/mother-erc20-native.json

status="$(curl -sS -o /tmp/mother-erc20-native.out.json -w '%{http_code}' \
  -X POST "$IB_API/v1/erc20-transfers/search" \
  -H "$JSON_HEADER" \
  -d @/tmp/mother-erc20-native.json)"
echo "HTTP $status"
jq -e '.ok == false and .error.code == "asset_not_erc20_on_network"' \
  /tmp/mother-erc20-native.out.json
```

### 7. Unknown Slug Rejection

Expected: HTTP `404`, `asset_not_found`.

```bash
jq '.tokens = {asset_slugs: ["missing-but-syntactically-valid"], contract_addresses: []}' \
  /tmp/mother-erc20-unfiltered.json > /tmp/mother-erc20-unknown-slug.json

status="$(curl -sS -o /tmp/mother-erc20-unknown-slug.out.json -w '%{http_code}' \
  -X POST "$IB_API/v1/erc20-transfers/search" \
  -H "$JSON_HEADER" \
  -d @/tmp/mother-erc20-unknown-slug.json)"
echo "HTTP $status"
jq -e '.ok == false and .error.code == "asset_not_found"' \
  /tmp/mother-erc20-unknown-slug.out.json
```

### 8. Too-Large Window Rejection

Expected: HTTP `422`, `window_too_large`.

```bash
jq --argjson from "$TEST_FROM_BLOCK" \
  '.window = {from_block: $from, to_block: ($from + 6000)}' \
  /tmp/mother-erc20-unfiltered.json > /tmp/mother-erc20-too-large-window.json

status="$(curl -sS -o /tmp/mother-erc20-too-large-window.out.json -w '%{http_code}' \
  -X POST "$IB_API/v1/erc20-transfers/search" \
  -H "$JSON_HEADER" \
  -d @/tmp/mother-erc20-too-large-window.json)"
echo "HTTP $status"
jq -e '.ok == false and .error.code == "window_too_large"' \
  /tmp/mother-erc20-too-large-window.out.json
```

### 9. Too-Many-Token-Filters Rejection

Expected: HTTP `422`, `too_many_token_filters`.

```bash
filters="$(seq 1 21 | awk '{printf "%s\"0x%040x\"", sep, $1; sep=","}')"
jq --argjson filters "[$filters]" \
  '.tokens = {asset_slugs: [], contract_addresses: $filters}' \
  /tmp/mother-erc20-unfiltered.json > /tmp/mother-erc20-too-many-filters.json

status="$(curl -sS -o /tmp/mother-erc20-too-many-filters.out.json -w '%{http_code}' \
  -X POST "$IB_API/v1/erc20-transfers/search" \
  -H "$JSON_HEADER" \
  -d @/tmp/mother-erc20-too-many-filters.json)"
echo "HTTP $status"
jq -e '.ok == false and .error.code == "too_many_token_filters"' \
  /tmp/mother-erc20-too-many-filters.out.json
```

### 10. Provider Timeout Or Disabled Extraction

Do not force provider timeouts in production. If a valid request from checks
2-5 fails, classify it with this command.

Expected classifications:

- HTTP `404` with no JSON body: Mother route gate is disabled.
- HTTP `503`, `extraction_unavailable`: Bigwig extraction is disabled,
  unconfigured, unreachable, or unavailable.
- HTTP `504`, `upstream_provider_timeout`: upstream RPC provider timed out.
- HTTP `504`, `extraction_timeout`: Bigwig exceeded its overall extraction
  deadline.

```bash
status="$(curl -sS -o /tmp/mother-erc20-diagnose.out.json -w '%{http_code}' \
  -X POST "$IB_API/v1/erc20-transfers/search" \
  -H "$JSON_HEADER" \
  -d @/tmp/mother-erc20-usdc-slug.json)"
echo "HTTP $status"
if [ "$status" = "200" ]; then
  echo "healthy"
elif [ -s /tmp/mother-erc20-diagnose.out.json ]; then
  jq -r '.error.code? // "unknown-json-body"' /tmp/mother-erc20-diagnose.out.json
else
  echo "no-json-body"
fi
```

## Operational Notes

- Balance routes require the Mother catalog and can return
  `503 asset_network_map_unavailable` if the catalog is unavailable.
- Supported balance items with unavailable Bigwig or Price Indexer dependencies
  remain `200 OK` responses with sanitized item-level errors.
- Unsupported asset-network pairs are skipped and reported in `skipped[]`.
- Bigwig Hub extraction must be enabled before Mother keeps the public gate on.
- Public block windows are capped at 5,000 inclusive blocks on `eth-mainnet`.
- Public lookback windows are capped at 86,400 seconds.
- Public token filters are capped at 20 unique contracts after asset-slug
  resolution, normalization, merge, and deduplication.
- `BIGWIG_REQUEST_TIMEOUT_MS=30000` matches the expected synchronous extraction
  deadline.
- Detect truncation with `jq '.limits.truncated' response.json`. `true` is a
  valid response, but it was capped by the upstream row limit.
- To narrow a query, reduce the block span, split the time range, add
  `asset_slugs`, or add explicit `contract_addresses`.
- Logs for failures should be sanitized. They must not include bearer tokens,
  `INFRA_GATEWAY_TOKEN`, provider URLs, raw upstream diagnostics, or Bigwig
  route internals.

## Pass Condition

The balance Beta gate passes when balance checks B1-B4 match their expected
HTTP status and JSON shape.

The optional transfer-search gate passes when transfer checks 1-9 match their
expected HTTP status and JSON shape, and transfer check 10 does not report
`extraction_unavailable`, `upstream_provider_timeout`, or `extraction_timeout`
for the valid USDC smoke payload.
