#!/usr/bin/env bash

set -Eeuo pipefail

umask 077

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"

container_name="${CONTAINER_NAME:-ibdb-postgres}"
backup_dir="${BACKUP_DIR:-${repo_root}/backups}"
temporary_backup=""

cleanup() {
  if [[ -n "${temporary_backup}" && -f "${temporary_backup}" ]]; then
    rm -f "${temporary_backup}"
  fi
}
trap cleanup EXIT

fail() {
  printf 'backup_now.sh: %s\n' "$*" >&2
  exit 1
}

command -v docker >/dev/null 2>&1 || fail "docker is required"

container_running="$(
  docker inspect --format '{{.State.Running}}' "${container_name}" 2>/dev/null || true
)"
[[ "${container_running}" == "true" ]] \
  || fail "Postgres container '${container_name}' is not running"

container_database="$(
  docker exec "${container_name}" printenv POSTGRES_DB 2>/dev/null || true
)"
container_user="$(
  docker exec "${container_name}" printenv POSTGRES_USER 2>/dev/null || true
)"

database_name="${POSTGRES_DB:-${container_database:-ibdb}}"
database_user="${POSTGRES_USER:-${container_user:-postgres}}"

docker exec "${container_name}" pg_isready \
  --username="${database_user}" \
  --dbname="${database_name}" >/dev/null \
  || fail "database '${database_name}' is not ready"

schema_list="$(
  docker exec "${container_name}" psql \
    --username="${database_user}" \
    --dbname="${database_name}" \
    --no-password \
    --no-psqlrc \
    --tuples-only \
    --no-align \
    --command="select nspname
               from pg_namespace
               where nspname <> 'information_schema'
                 and nspname !~ '^pg_'
               order by nspname"
)" || fail "could not list schemas in database '${database_name}'"

[[ -n "${schema_list}" ]] \
  || fail "database '${database_name}' has no user schemas to back up"

mkdir -p "${backup_dir}"

timestamp="$(date -u '+%Y%m%dT%H%M%SZ')"
backup_path="${backup_dir}/${database_name}-${timestamp}-$$.dump"
temporary_backup="$(mktemp "${backup_path}.tmp.XXXXXX")" \
  || fail "could not create a temporary backup file"

printf 'Backing up PostgreSQL database %s from %s.\n' \
  "${database_name}" "${container_name}"
printf 'User schemas included:\n'
while IFS= read -r schema_name; do
  printf '  - %s\n' "${schema_name}"
done <<< "${schema_list}"

# Omitting --schema is intentional: pg_dump includes every user schema,
# including schemas added to IBDB by other Iron Burrow services.
docker exec "${container_name}" pg_dump \
  --username="${database_user}" \
  --dbname="${database_name}" \
  --no-password \
  --format=custom \
  --compress=9 > "${temporary_backup}" \
  || fail "pg_dump failed"

[[ -s "${temporary_backup}" ]] || fail "pg_dump produced an empty archive"

docker exec --interactive "${container_name}" pg_restore --list \
  < "${temporary_backup}" >/dev/null \
  || fail "pg_restore could not validate the archive"

mv "${temporary_backup}" "${backup_path}"
temporary_backup=""

printf 'Backup complete: %s\n' "${backup_path}"
