#!/usr/bin/env sh
set -eu

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
REPO_ROOT="$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)"

SMOKE_RUN_ID="${SMOKE_RUN_ID:-$$}"
SMOKE_NETWORK="mother-api-smoke-db-lifecycle-net-${SMOKE_RUN_ID}"
SMOKE_DB_CONTAINER="mother-api-smoke-db-lifecycle-postgres-${SMOKE_RUN_ID}"
SMOKE_API_CONTAINER="mother-api-smoke-db-lifecycle-api-${SMOKE_RUN_ID}"
SMOKE_IMAGE="mother-api-smoke-db-lifecycle:${SMOKE_RUN_ID}"
SMOKE_DB_NAME="mother_api_smoke_lifecycle_test_${SMOKE_RUN_ID}"
SMOKE_DATABASE_URL="postgres://postgres:postgres@${SMOKE_DB_CONTAINER}:5432/${SMOKE_DB_NAME}"

cleanup() {
	docker rm -f "$SMOKE_API_CONTAINER" >/dev/null 2>&1 || true
	docker rm -f "$SMOKE_DB_CONTAINER" >/dev/null 2>&1 || true
	docker network rm "$SMOKE_NETWORK" >/dev/null 2>&1 || true
	docker image rm "$SMOKE_IMAGE" >/dev/null 2>&1 || true
}

if ! command -v docker >/dev/null 2>&1; then
	echo "Docker is required for make smoke-db-lifecycle. Install Docker and try again." >&2
	exit 1
fi

if ! docker info >/dev/null 2>&1; then
	echo "Docker is required for make smoke-db-lifecycle, but the Docker daemon is not available. Start Docker and try again." >&2
	exit 1
fi

cd "$REPO_ROOT"

trap cleanup EXIT INT TERM

cleanup

echo "Starting disposable Postgres database '${SMOKE_DB_NAME}'..."

docker network create "$SMOKE_NETWORK" >/dev/null

docker run --rm -d \
	--name "$SMOKE_DB_CONTAINER" \
	--network "$SMOKE_NETWORK" \
	-e POSTGRES_DB="$SMOKE_DB_NAME" \
	-e POSTGRES_USER=postgres \
	-e POSTGRES_PASSWORD=postgres \
	postgres:17-alpine >/dev/null

ready=0

for attempt in $(seq 1 60); do
	if docker run --rm --network "$SMOKE_NETWORK" postgres:17-alpine \
		pg_isready -h "$SMOKE_DB_CONTAINER" -U postgres -d "$SMOKE_DB_NAME" >/dev/null 2>&1; then
		ready=1
		break
	fi

	sleep 1
done

if [ "$ready" != "1" ]; then
	echo "Timed out waiting for disposable Postgres to become healthy." >&2
	exit 1
fi

echo "Building Mother API smoke image '${SMOKE_IMAGE}'..."

docker build \
	-f infra/docker/Dockerfile.mother-api \
	-t "$SMOKE_IMAGE" \
	.

echo "Asserting production image does not include sqlx-cli or psql..."

docker run --rm \
	--entrypoint sh \
	"$SMOKE_IMAGE" \
	-c 'if command -v sqlx >/dev/null 2>&1; then echo "sqlx must not be present" >&2; exit 1; fi; if command -v psql >/dev/null 2>&1; then echo "psql must not be present" >&2; exit 1; fi'

echo "Running full database lifecycle through mother-api db apply..."

docker run --rm \
	--network "$SMOKE_NETWORK" \
	-e DATABASE_URL="$SMOKE_DATABASE_URL" \
	"$SMOKE_IMAGE" \
	mother-api db apply

snapshot_catalog_rows() {
	docker run --rm \
		--network "$SMOKE_NETWORK" \
		-e PGPASSWORD=postgres \
		postgres:17-alpine \
		psql \
		-h "$SMOKE_DB_CONTAINER" \
		-U postgres \
		-d "$SMOKE_DB_NAME" \
		-v ON_ERROR_STOP=1 \
		-At \
		-F '|' \
		-c "
			select 'asset', id::text, created_at::text, updated_at::text
			from mother_api.global_asset
			where slug = 'bitso-mxn'
			union all
			select 'network', id::text, created_at::text, updated_at::text
			from mother_api.network
			where slug = 'arbitrum-mainnet'
			union all
			select 'mapping', mapping.id::text, mapping.created_at::text, mapping.updated_at::text
			from mother_api.asset_chain_map mapping
			join mother_api.global_asset asset on asset.id = mapping.asset_id
			join mother_api.network network on network.id = mapping.network_id
			where asset.slug = 'bitso-mxn'
				and network.slug = 'arbitrum-mainnet'
			order by 1;
		"
}

before_snapshot="$(snapshot_catalog_rows)"

if [ "$(printf '%s\n' "$before_snapshot" | sed '/^$/d' | wc -l | tr -d ' ')" != "3" ]; then
	echo "Expected lifecycle smoke snapshot to include asset, network, and mapping rows." >&2
	printf '%s\n' "$before_snapshot" >&2
	exit 1
fi

echo "Running mother-api db apply a second time to prove no-op behavior..."

docker run --rm \
	--network "$SMOKE_NETWORK" \
	-e DATABASE_URL="$SMOKE_DATABASE_URL" \
	"$SMOKE_IMAGE" \
	mother-api db apply

after_snapshot="$(snapshot_catalog_rows)"

if [ "$after_snapshot" != "$before_snapshot" ]; then
	echo "Second db apply changed reference-data audit rows." >&2
	echo "Before:" >&2
	printf '%s\n' "$before_snapshot" >&2
	echo "After:" >&2
	printf '%s\n' "$after_snapshot" >&2
	exit 1
fi

echo "Starting mother-api serve from the same image..."

docker run --rm -d \
	--name "$SMOKE_API_CONTAINER" \
	--network "$SMOKE_NETWORK" \
	-e DATABASE_URL="$SMOKE_DATABASE_URL" \
	"$SMOKE_IMAGE" \
	mother-api serve >/dev/null

healthy=0

for attempt in $(seq 1 60); do
	if docker run --rm \
		--network "$SMOKE_NETWORK" \
		"$SMOKE_IMAGE" \
		wget -qO- "http://${SMOKE_API_CONTAINER}:3000/health" >/dev/null 2>&1; then
		healthy=1
		break
	fi

	sleep 1
done

if [ "$healthy" != "1" ]; then
	echo "Timed out waiting for mother-api serve to answer /health." >&2
	docker logs "$SMOKE_API_CONTAINER" >&2 || true
	exit 1
fi

echo "Database lifecycle smoke test passed against disposable Docker Postgres."
