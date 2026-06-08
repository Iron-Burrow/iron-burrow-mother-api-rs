#!/usr/bin/env bash
set -euo pipefail

NETWORKS=(
  "iron-burrow-net"
  "iron-burrow-public-net"
  "iron-burrow-infra-net"
)

if ! command -v docker >/dev/null 2>&1; then
  printf 'Error: docker is required.\n' >&2
  exit 1
fi

for network_name in "${NETWORKS[@]}"; do
  if [ -z "$network_name" ]; then
    printf 'Error: network name must not be empty.\n' >&2
    exit 1
  fi

  if docker network inspect "$network_name" >/dev/null 2>&1; then
    printf 'Docker network already exists: %s\n' "$network_name"
  else
    docker network create "$network_name" >/dev/null
    printf 'Created Docker network: %s\n' "$network_name"
  fi
done
