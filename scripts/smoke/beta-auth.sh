#!/usr/bin/env sh
set -eu

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
REPO_ROOT="$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)"

SMOKE_RUN_ID="${SMOKE_RUN_ID:-$$}"
SMOKE_DB_CONTAINER="mother-api-smoke-beta-auth-postgres-${SMOKE_RUN_ID}"
SMOKE_DB_NAME="mother_api_smoke_beta_auth_${SMOKE_RUN_ID}"
SMOKE_HTTP_PORT="${SMOKE_HTTP_PORT:-3101}"
SMOKE_TMP_PREFIX="${TMPDIR:-/tmp}/mother-api-smoke-beta-auth-${SMOKE_RUN_ID}"
SMOKE_API_LOG="${SMOKE_TMP_PREFIX}.log"
SMOKE_DATABASE_URL=""
SMOKE_API_PID=""

cleanup() {
	if [ -n "$SMOKE_API_PID" ]; then
		kill "$SMOKE_API_PID" >/dev/null 2>&1 || true
		wait "$SMOKE_API_PID" >/dev/null 2>&1 || true
	fi
	docker rm -f "$SMOKE_DB_CONTAINER" >/dev/null 2>&1 || true
	rm -f "${SMOKE_TMP_PREFIX}"* >/dev/null 2>&1 || true
}

fail() {
	echo "$1" >&2
	if [ -f "$SMOKE_API_LOG" ]; then
		echo "Mother API smoke log:" >&2
		sed 's/\(api_key\|authorization\)[^"]*/<redacted>/Ig' "$SMOKE_API_LOG" >&2 || true
	fi
	exit 1
}

require_command() {
	if ! command -v "$1" >/dev/null 2>&1; then
		echo "$1 is required for make smoke-beta-auth. Install it and try again." >&2
		exit 1
	fi
}

assert_status() {
	actual="$1"
	expected="$2"
	label="$3"

	if [ "$actual" != "$expected" ]; then
		fail "$label returned HTTP $actual, expected $expected."
	fi
}

assert_no_secret() {
	output="$1"
	secret="$2"
	label="$3"

	if printf '%s\n' "$output" | grep -F "$secret" >/dev/null 2>&1; then
		fail "$label output included the raw API key."
	fi

	if printf '%s\n' "$output" | grep -F "key_hash" >/dev/null 2>&1; then
		fail "$label output included key_hash."
	fi
}

require_command cargo
require_command curl
require_command docker
require_command jq
require_command seq

if ! docker info >/dev/null 2>&1; then
	echo "Docker is required for make smoke-beta-auth, but the Docker daemon is not available. Start Docker and try again." >&2
	exit 1
fi

cd "$REPO_ROOT"

trap cleanup EXIT INT TERM

cleanup

echo "Starting disposable Postgres database '${SMOKE_DB_NAME}'..."

docker run --rm -d \
	--name "$SMOKE_DB_CONTAINER" \
	-p 127.0.0.1::5432 \
	-e POSTGRES_DB="$SMOKE_DB_NAME" \
	-e POSTGRES_USER=postgres \
	-e POSTGRES_PASSWORD=postgres \
	postgres:17-alpine >/dev/null

host_port="$(docker port "$SMOKE_DB_CONTAINER" 5432/tcp | sed 's/.*://')"

if [ -z "$host_port" ]; then
	fail "Docker did not publish a localhost Postgres port for ${SMOKE_DB_CONTAINER}."
fi

ready=0

for attempt in $(seq 1 60); do
	if docker exec "$SMOKE_DB_CONTAINER" \
		pg_isready -U postgres -d "$SMOKE_DB_NAME" >/dev/null 2>&1; then
		ready=1
		break
	fi

	sleep 1
done

if [ "$ready" != "1" ]; then
	fail "Timed out waiting for disposable Postgres to become healthy."
fi

SMOKE_DATABASE_URL="postgres://postgres:postgres@127.0.0.1:${host_port}/${SMOKE_DB_NAME}"
SMOKE_API="http://127.0.0.1:${SMOKE_HTTP_PORT}"

echo "Applying embedded migrations and reference data..."

DATABASE_URL="$SMOKE_DATABASE_URL" \
	cargo run --quiet --bin mother-api -- db apply

echo "Starting local Mother API in beta mode on ${SMOKE_API}..."

APP_ENV=smoke \
PUBLIC_API_SURFACE=beta \
HTTP_HOST=127.0.0.1 \
HTTP_PORT="$SMOKE_HTTP_PORT" \
DATABASE_URL="$SMOKE_DATABASE_URL" \
cargo run --quiet --bin mother-api -- serve >"$SMOKE_API_LOG" 2>&1 &
SMOKE_API_PID="$!"

healthy=0

for attempt in $(seq 1 90); do
	if ! kill -0 "$SMOKE_API_PID" >/dev/null 2>&1; then
		fail "Mother API process exited before becoming healthy."
	fi

	if curl -sS "${SMOKE_API}/health" >/dev/null 2>&1; then
		healthy=1
		break
	fi

	sleep 1
done

if [ "$healthy" != "1" ]; then
	fail "Timed out waiting for Mother API to answer /health."
fi

payload="${SMOKE_TMP_PREFIX}.json"
cat >"$payload" <<'JSON'
{
  "as_of": {
    "kind": "latest"
  },
  "account": {
    "network_slug": "eth-mainnet",
    "address": "0x1234567890abcdef1234567890abcdef1234beef",
    "chain": "eth-mainnet"
  },
  "quote_currency": "USD",
  "assets": [
    {
      "asset_slug": "ethereum"
    }
  ]
}
JSON

echo "Checking /health remains public..."

curl -sS "${SMOKE_API}/health" \
	| jq -e '.ok == true and .service == "iron-burrow-mother-api"' >/dev/null

echo "Checking protected route rejects missing API key..."

status="$(curl -sS -o "${payload}.no-key.out" -w '%{http_code}' \
	-X POST "${SMOKE_API}/v1/balances" \
	-H 'Content-Type: application/json' \
	-d @"$payload")"
assert_status "$status" "401" "Protected route without API key"
jq -e '.ok == false and .error.code == "unauthorized"' "${payload}.no-key.out" >/dev/null

echo "Issuing throwaway API key with a one-request daily limit..."

issue_output="$(DATABASE_URL="$SMOKE_DATABASE_URL" cargo run --quiet --bin mother-api -- admin api-key issue \
	--consumer-slug beta-auth-smoke \
	--display-name "Beta Auth Smoke" \
	--category internal \
	--label "disposable beta auth smoke key" \
	--requests-per-minute 60 \
	--requests-per-day 1 \
	--format json)"

api_key="$(printf '%s\n' "$issue_output" | jq -r '.api_key')"
key_prefix="$(printf '%s\n' "$issue_output" | jq -r '.key_prefix')"
auth_header="Authorization: Bearer ${api_key}"

if [ -z "$api_key" ] || [ "$api_key" = "null" ]; then
	fail "API-key issue command did not return api_key."
fi

echo "Checking valid API key reaches protected-route validation..."

status="$(curl -sS -o "${payload}.valid.out" -w '%{http_code}' \
	-X POST "${SMOKE_API}/v1/balances" \
	-H 'Content-Type: application/json' \
	-H "$auth_header" \
	-d @"$payload")"
assert_status "$status" "400" "Protected route with valid API key"
jq -e '.ok == false and .error.code == "invalid_request"' "${payload}.valid.out" >/dev/null

echo "Checking tiny daily limit returns rate_limited..."

status="$(curl -sS -o "${payload}.limited.out" -w '%{http_code}' \
	-X POST "${SMOKE_API}/v1/balances" \
	-H 'Content-Type: application/json' \
	-H "$auth_header" \
	-d @"$payload")"
assert_status "$status" "429" "Protected route over daily API-key limit"
jq -e '.ok == false and .error.code == "rate_limited"' "${payload}.limited.out" >/dev/null

echo "Revoking throwaway API key..."

revoke_output="$(DATABASE_URL="$SMOKE_DATABASE_URL" cargo run --quiet --bin mother-api -- admin api-key revoke \
	--key-prefix "$key_prefix" \
	--format json)"
printf '%s\n' "$revoke_output" \
	| jq -e '.ok == true and .status == "revoked"' >/dev/null
assert_no_secret "$revoke_output" "$api_key" "revoke"

echo "Checking revoked key returns unauthorized..."

status="$(curl -sS -o "${payload}.revoked.out" -w '%{http_code}' \
	-X POST "${SMOKE_API}/v1/balances" \
	-H 'Content-Type: application/json' \
	-H "$auth_header" \
	-d @"$payload")"
assert_status "$status" "401" "Protected route with revoked API key"
jq -e '.ok == false and .error.code == "unauthorized"' "${payload}.revoked.out" >/dev/null

echo "Inspecting API-key list and usage without leaking raw keys..."

list_output="$(DATABASE_URL="$SMOKE_DATABASE_URL" cargo run --quiet --bin mother-api -- admin api-key list \
	--consumer-slug beta-auth-smoke \
	--format json)"
printf '%s\n' "$list_output" \
	| jq -e --arg key_prefix "$key_prefix" '.ok == true and any(.keys[]; .key_prefix == $key_prefix and .status == "revoked")' >/dev/null
assert_no_secret "$list_output" "$api_key" "list"

usage_output="$(DATABASE_URL="$SMOKE_DATABASE_URL" cargo run --quiet --bin mother-api -- admin api-key usage \
	--consumer-slug beta-auth-smoke \
	--days 1 \
	--format json)"
printf '%s\n' "$usage_output" \
	| jq -e --arg key_prefix "$key_prefix" '.ok == true and any(.usage[]; .key_prefix == $key_prefix and .accepted_requests >= 1 and .rate_limited_requests >= 1)' >/dev/null
assert_no_secret "$usage_output" "$api_key" "usage"

echo "Beta API-key auth smoke test passed against disposable local Postgres."
