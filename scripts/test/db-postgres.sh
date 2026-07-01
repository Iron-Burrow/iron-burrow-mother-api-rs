#!/usr/bin/env sh
set -eu

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
REPO_ROOT="$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)"

TEST_DB_CONTAINER="mother-api-test-db-postgres-$$"
TEST_DB_NAME="mother_api_postgres_regression_test"

cleanup() {
	docker rm -f "$TEST_DB_CONTAINER" >/dev/null 2>&1 || true
}

if ! command -v docker >/dev/null 2>&1; then
	echo "Docker is required for make test-db-postgres. Install Docker and try again." >&2
	exit 1
fi

if ! docker info >/dev/null 2>&1; then
	echo "Docker is required for make test-db-postgres, but the Docker daemon is not available. Start Docker and try again." >&2
	exit 1
fi

cd "$REPO_ROOT"

trap cleanup EXIT INT TERM

cleanup

echo "Starting disposable Postgres database '${TEST_DB_NAME}'..."

docker run --rm -d \
	--name "$TEST_DB_CONTAINER" \
	-p 127.0.0.1::5432 \
	-e POSTGRES_DB="$TEST_DB_NAME" \
	-e POSTGRES_USER=postgres \
	-e POSTGRES_PASSWORD=postgres \
	postgres:17-alpine >/dev/null

host_port="$(docker port "$TEST_DB_CONTAINER" 5432/tcp | sed 's/.*://')"

if [ -z "$host_port" ]; then
	echo "Docker did not publish a localhost Postgres port for ${TEST_DB_CONTAINER}." >&2
	exit 1
fi

ready=0

for attempt in $(seq 1 60); do
	if docker exec "$TEST_DB_CONTAINER" \
		pg_isready -U postgres -d "$TEST_DB_NAME" >/dev/null 2>&1; then
		ready=1
		break
	fi

	sleep 1
done

if [ "$ready" != "1" ]; then
	echo "Timed out waiting for disposable Postgres to become healthy." >&2
	exit 1
fi

echo "Running Rust Postgres-backed regression tests..."

MOTHER_API_POSTGRES_TEST_DATABASE_URL="postgres://postgres:postgres@127.0.0.1:${host_port}/${TEST_DB_NAME}" \
	cargo test adapters::postgres::tests -- --test-threads=1

MOTHER_API_POSTGRES_TEST_DATABASE_URL="postgres://postgres:postgres@127.0.0.1:${host_port}/${TEST_DB_NAME}" \
	cargo test reference_data::tests -- --test-threads=1

echo "Rust Postgres-backed regression tests passed against disposable Docker Postgres."
