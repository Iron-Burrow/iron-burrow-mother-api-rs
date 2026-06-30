#!/usr/bin/env sh
set -eu

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
REPO_ROOT="$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)"

SMOKE_NETWORK="mother-api-smoke-db-migration-net"
SMOKE_DB_CONTAINER="mother-api-smoke-db-migration-postgres"
SMOKE_IMAGE="mother-api-smoke-db-migration"
SMOKE_DB_NAME="mother_api_smoke_migration_test"
SMOKE_DATABASE_URL="postgres://postgres:postgres@${SMOKE_DB_CONTAINER}:5432/${SMOKE_DB_NAME}"

cleanup() {
	docker rm -f "$SMOKE_DB_CONTAINER" >/dev/null 2>&1 || true
	docker network rm "$SMOKE_NETWORK" >/dev/null 2>&1 || true
	docker image rm "$SMOKE_IMAGE" >/dev/null 2>&1 || true
}

if ! command -v docker >/dev/null 2>&1; then
	echo "Docker is required for make smoke-db-migrate. Install Docker and try again." >&2
	exit 1
fi

if ! docker info >/dev/null 2>&1; then
	echo "Docker is required for make smoke-db-migrate, but the Docker daemon is not available. Start Docker and try again." >&2
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

echo "Running embedded migrations through mother-api db migrate..."

docker run --rm \
	--network "$SMOKE_NETWORK" \
	-e DATABASE_URL="$SMOKE_DATABASE_URL" \
	"$SMOKE_IMAGE" \
	mother-api db migrate

echo "Embedded migration smoke test passed against disposable Docker Postgres."