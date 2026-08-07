---
status: draft
owner: iron-burrow
last_reviewed: 2026-08-07
agent_edit_policy: update_when_relevant
---

# RFC-005 - Incremental Mother-Owned SQLite Persistence

## Status

Draft proposal. This RFC changes no runtime behavior, database lifecycle,
public route, OpenAPI operation, or binding contract until it is accepted and
implemented by a focused follow-up specification.

## Summary

Mother API should add a Mother-owned SQLite persistence store through SQLx and
move bounded application-owned domains into it incrementally. The initial
domain is the canonical registry: global assets, networks, asset/network
mappings, protocol registrations, and verified protocol targets and
configuration.

This is not a one-step migration away from PostgreSQL. PostgreSQL remains
supported for all unmigrated state, including accounts, sessions, API keys,
quotas, workspaces, and other existing PostgreSQL-backed behavior. The
proposal does not create a local-runtime profile, automatically modify
database state at `serve` startup, introduce mock infrastructure, or change a
public API contract.

## Motivation

Mother owns a small, relatively static canonical registry that is required to
resolve supported assets, networks, deployed addresses, and verified protocol
configuration. Today, those registry records are persisted in PostgreSQL:

- `mother_api.global_asset`;
- `mother_api.network`;
- `mother_api.asset_chain_map`;
- `mother_api.defi_protocol`; and
- `mother_api.defi_protocol_target`.

The embedded `reference-data/catalog.json` already declares assets, networks,
and asset/network mappings. Protocol registration is currently reconciled by
PostgreSQL-specific code. SQLx is presently configured for PostgreSQL only,
and the existing migrations, reference-data writes, and repository queries use
PostgreSQL-specific schemas and syntax.

Replacing all persistence would add migration risk without an immediate
benefit. Moving only the registry establishes a portable, runtime-local owner
for data that Mother already owns, while retaining PostgreSQL for state that
requires its existing behavior and operational guarantees.

## Decision requested

Adopt incremental SQLite ownership for Mother's canonical registry.

For each registry slice that is migrated, SQLite is the authoritative runtime
read store. Mother must not fall back to PostgreSQL for that slice. A missing,
unprepared, unavailable, or invalid SQLite registry is a startup or request
failure to be reported clearly by the implementation; it is not a reason to
silently read stale PostgreSQL registry data.

PostgreSQL registry tables may remain during the transition for compatibility
with unmigrated code and existing deployments. Their presence does not make
them a runtime read fallback after a given registry slice has moved.

## Initial ownership boundary

The initial SQLite-owned domain contains only the following canonical,
application-owned data:

| Data | Canonical identity | Purpose |
| --- | --- | --- |
| Global asset | normalized asset slug | Asset name, symbol, aliases, status, and ordering. |
| Network | `network_slug` | Supported network name, family, status, ordering, CAIP-2 data, and EIP-155 `chain_id` where applicable. |
| Asset/network mapping | asset slug + `network_slug` | Native or deployed asset representation, token standard, decimals, deployment address, and deployment block. |
| Protocol registration | normalized protocol slug + `network_slug` | Verified, enabled compiled-adapter registration and its versioned configuration. |
| Protocol target | protocol identity + stable target key | Verified pool or reserve target, address, and its bound asset/network mapping when applicable. |

`network_slug` remains the canonical supported network identity. A numeric
`chain_id` remains distinct and is used only when it means an EIP-155 chain
identifier; it is not a generic replacement for `network_slug`.

The boundary does not move account, session, API-key, capability-grant, quota,
workspace, portfolio, usage, or other mutable application records into
SQLite. It does not make a registry row executable by itself: compiled Mother
adapters continue to own ABI, calldata, decoding, and supported protocol
behavior.

## Registry source and bootstrap

The embedded versioned catalog remains the declarative source for the initial
registry. A follow-up implementation must extend it with explicit protocol
registrations and targets, including the validated references needed to bind a
target to a network and, where required, to an asset/network mapping. It must
replace PostgreSQL-only hard-coded protocol seeding with that declaration; the
catalog must remain the reviewable source rather than a database dump.

SQLite schema creation and catalog application are separate concerns. The
future implementation must provide a dedicated SQLite schema and an
idempotent bootstrap path that:

1. validates the complete catalog before writes;
2. creates or migrates the SQLite registry schema through SQLx;
3. applies canonical declarations by their natural identities;
4. updates only changed declarations; and
5. completes atomically or leaves no partial catalog application.

Reapplying unchanged declarations must preserve stable registry identities and
avoid unnecessary record churn. Ordinary lifecycle operations must not delete
local state outside this canonical registry scope.

## Lifecycle and runtime behavior

Accepted [SPEC-009](../specs/SPEC-009-reference-data-and-migration-lifecycle.md)
requires explicit database-state commands and states that `serve` never
applies database state implicitly. RFC-005 preserves that rule.

The focused implementation specification must define the SQLite command and
configuration interface, but it must require explicit preparation before
Mother serves any feature that depends on a migrated registry slice. `serve`
may open and validate the configured SQLite registry; it must not create it,
migrate it, or bootstrap it implicitly.

The implementation must add a dedicated SQLite schema and SQLx-backed
repository path. It must not claim PostgreSQL/SQLite interchangeability or
degrade PostgreSQL schema and query design solely for portability. Shared code
is appropriate only where behavior and SQLx abstractions are genuinely
portable.

## Compatibility and migration approach

Migration proceeds by bounded registry slice, not by a database-wide cutover.
For each slice, a focused implementation specification must identify the
repository interface, all runtime read call sites, the SQLite schema and
bootstrap declarations, and the PostgreSQL behavior that remains unmigrated.

At the point a slice becomes SQLite-owned, all of its runtime reads must use
the SQLite repository in the same implementation change. No dual-read,
PostgreSQL fallback, replication, production cutover, historical import, or
PostgreSQL schema deletion is implied by this RFC.

Existing PostgreSQL-backed behavior remains supported throughout the
transition. Its regression coverage remains required under the repository's
disposable PostgreSQL test path. Future domains may move only through their
own accepted specification after their ownership, lifecycle, and compatibility
requirements are established.

## Non-goals

This RFC does not:

- make SQLite a production replacement for all PostgreSQL state;
- alter PostgreSQL migrations or require importing all production data;
- add automatic migration or bootstrap during `mother-api serve`;
- introduce a local runtime profile, Docker-free product environment, mock
  Bigwig boundary, demo authentication, module toggles, or ephemeral mode;
- add or change `/v1` endpoints, browser routes, OpenAPI operations, or
  `CONTRACTS.md` promises;
- implement DeFi position discovery or make unverified protocol data
  executable; or
- change the ownership of price availability, derivation, FX, historical price
  data, chain reads, or refresh scheduling.

Draft [SPEC-024](../specs/SPEC-024-mother-owned-defi-position-discovery-and-search.md)
continues to govern the proposed DeFi position-discovery scope. This RFC only
addresses persistence for the canonical registry that a future accepted scope
may use.

## Future implementation requirements

A follow-up accepted specification must define the concrete SQLite lifecycle
command/configuration interface and demonstrate all of the following:

1. A fresh SQLite registry can be explicitly created, migrated, and populated
   from the validated embedded catalog.
2. Repeating preparation is idempotent, leaves unchanged records stable, and
   rejects invalid declarations without partial application.
3. Canonical asset and `network_slug` resolution returns the expected native
   or deployed representation; an EIP-155 `chain_id` is handled only as the
   numeric property it represents.
4. Verified protocol registration and target configuration resolve only
   declared network-consistent targets.
5. Every runtime read for a migrated slice uses SQLite and fails explicitly
   rather than falling back to PostgreSQL.
6. Existing PostgreSQL-backed behavior and its disposable-database regression
   suite continue to pass.
7. Existing public API contract and OpenAPI coverage remain unchanged unless a
   separately accepted feature changes them and updates `CONTRACTS.md`.

## Consequences

This RFC establishes a deliberate incremental persistence direction, not an
implementation. It narrows the first step to data Mother owns outright and
preserves the established PostgreSQL lifecycle for all other state. Review and
acceptance of this RFC authorize a focused implementation specification; they
do not themselves authorize dependency, schema, runtime, or public-contract
changes.
