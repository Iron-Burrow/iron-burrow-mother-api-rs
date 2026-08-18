---
status: accepted
owner: iron-burrow
last_reviewed: 2026-08-18
agent_edit_policy: update_when_relevant
---

# SPEC-034 - Binary-Owned Capability and Verified Protocol Registries

## Purpose

Move Mother’s bounded capability definitions and verified protocol facts into
release-embedded immutable registries. PostgreSQL remains the source of truth
for mutable authorization grants and all other mutable Mother data.

## Scope

- `CapabilityRegistry` owns valid IDs, descriptions, and compiled baseline
  sets. Persisted grants retain their `capability_id` columns but every
  application read and write validates against this registry.
- `VerifiedProtocolRegistry` owns the exact Aave V3 realized-yield deployment:
  `aave-v3` on `eth-mainnet`, its verified pool, and USDC, USDT, DAI, and GHO
  reserve targets.
- `reference-data/verified-protocols.json` is a separate embedded artifact;
  the canonical asset catalog remains limited to assets, networks, and maps.
- Realized-yield and portfolio-simulation static protocol resolution is
  synchronous and database-independent. Bigwig remains the blockchain-read
  boundary.

## PostgreSQL compatibility

Released `0009_legacy_api_key_capabilities.sql` remains unchanged. Migration
`0018_remove_database_capability_registry.sql` rejects unknown persisted
capability IDs, safely backfills legacy baseline grants without broadening a
revoked or narrower grant, removes all capability foreign keys, and then drops
`mother_api.capability`. Grant tables, grant rows, and `capability_id` columns
remain mutable PostgreSQL authorization data.

Historical `mother_api.defi_protocol` and `mother_api.defi_protocol_target`
tables and migration `0017_defi_protocol_registry.sql` remain intact, but are
not runtime sources. Mother retires `db apply-reference`; `db apply` runs
embedded migrations only.

## Validation and behavior

Both registries are constructed before application state and listener binding.
Protocol construction validates canonical active networks, assets, and maps;
lowercase EVM addresses; unique protocol/target identities; verified pool and
reserve shapes; deterministic reserve order; and compiled adapter kind/version
support. Any malformed declaration fails startup without partial state.

An unknown capability ID read from a persisted grant is a database-integrity
error. It must not be ignored or authorized. Anonymous demo authorization uses
the compiled legacy baseline scoped to `eth-mainnet`, without querying the
retired capability table.

## Non-goals

This specification adds no endpoint, OpenAPI operation, README or
`CONTRACTS.md` promise, protocol discovery, arbitrary calldata, direct RPC,
or DeFi position-discovery behavior. PostgreSQL continues to own
authentication, accounts, clients, Workspaces, quotas, usage, snapshots,
simulations, and mutable grants.

## Acceptance criteria

- Embedded-registry tests run without PostgreSQL and prove the exact Aave
  configuration plus invalid configuration rejection.
- Route/service tests prove static capability and protocol resolution does not
  query PostgreSQL while mutable authorization and persistence still do.
- Postgres migration tests cover 0018 rejection, valid grant persistence,
  legacy backfill preservation, and a v0.3.0-shaped upgrade.
- Production source has no runtime path to `mother_api.capability`,
  `mother_api.defi_protocol`, or `mother_api.defi_protocol_target`.
- `cargo fmt`, `cargo test`, `make test-db-postgres`, `make smoke-db-migrate`,
  and `git diff --check` pass before `HISTORY.md` is appended.
