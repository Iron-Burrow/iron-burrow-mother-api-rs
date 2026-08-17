---
status: superseded
owner: iron-burrow
last_reviewed: 2026-08-17
agent_edit_policy: do_not_update
superseded_by: docs/rfcs/RFC-006-in-memory-canonical-registry.md
---

> Archived: 2026-08-17
>
> Lifecycle: Superseded
>
> Reason: Accepted [RFC-006](../rfcs/RFC-006-in-memory-canonical-registry.md) replaces this SQLite registry direction with an in-memory `CanonicalRegistry`.
>
> Evidence:
> - RFC-006 explicitly states that it supersedes RFC-005 and does not authorize SQLite work.
> - The current repository constructs `CanonicalRegistry` from the embedded catalog and has no Mother-owned SQLite lifecycle, configuration, or runtime store.
>
> Notes:
> - Historical context is preserved; no public API contract changes.

# RFC-005 - Incremental Mother-Owned SQLite Persistence

## Status

Accepted architectural direction. This RFC does not itself change runtime
behavior, database lifecycle, public routes, OpenAPI operations, or binding
contracts. Each registry slice changes only through its own accepted and
implemented specification.

## Summary

Mother API will add a Mother-owned SQLite persistence store through SQLx and
move bounded application-owned data into it incrementally. The canonical
registry moves in two ordered slices:

1. global assets, networks, and asset/network mappings; then
2. verified protocol registrations, targets, and versioned configuration.

This is not a migration away from PostgreSQL. PostgreSQL remains the store for
all mutable application state and every unmigrated behavior, including
accounts, sessions, API keys, quotas, workspaces, portfolio records, and
usage. The decision does not create a local-runtime profile, automatically
modify database state at `serve` startup, introduce mock infrastructure, or
change a public API contract.

## Decision

Adopt incremental SQLite ownership for Mother's canonical registry.

For each migrated registry slice, SQLite is the authoritative runtime read
store. Mother must not fall back to PostgreSQL for that slice. A missing,
unprepared, unavailable, stale, or invalid SQLite registry is a clear startup
or request failure; it is never a reason to read PostgreSQL registry rows.

PostgreSQL registry tables may remain during the transition where an
unmigrated slice still needs them. Their presence does not make them a runtime
read fallback after a slice has moved.

## Ownership boundary and slices

The SQLite registry contains only the following canonical, application-owned
data:

| Data | Canonical identity | Purpose |
| --- | --- | --- |
| Global asset | normalized asset slug | Asset name, symbol, aliases, status, and ordering. |
| Network | `network_slug` | Supported network name, family, status, ordering, CAIP-2 data, and EIP-155 `chain_id` where applicable. |
| Asset/network mapping | asset slug + `network_slug` | Native or deployed representation, token standard, decimals, deployment address, and deployment block. |
| Protocol registration | normalized protocol slug + `network_slug` | Verified, enabled compiled-adapter registration and versioned configuration. |
| Protocol target | protocol identity + stable target key | Verified pool or reserve target, address, and its bound asset/network mapping when applicable. |

The first slice is `global_asset`, `network`, and `asset_chain_map`. The second
slice is `defi_protocol` and `defi_protocol_target`; it depends on the first
because target validation can bind a target to an asset/network mapping.

`network_slug` remains the canonical supported-network identity. A numeric
`chain_id` remains distinct and is used only when it means an EIP-155 chain
identifier; it is not a generic replacement for `network_slug`.

The boundary does not move accounts, sessions, API keys, capability grants,
quotas, workspaces, portfolios, usage, or other mutable records into SQLite.
Capabilities remain PostgreSQL reference data. A registry row does not become
executable by itself: compiled Mother adapters continue to own ABI, calldata,
decoding, and supported protocol behavior.

## Registry source and bootstrap

The embedded versioned catalog remains the declarative source for the registry.
Its existing `assets`, `networks`, and `asset_chain_maps` declarations are the
source for the first slice. A follow-up implementation extends it with explicit
protocol registrations and targets, including validated network and
asset/network mapping references. It replaces PostgreSQL-only hard-coded
protocol seeding; the catalog remains reviewable source data, not a database
dump.

SQLite schema creation and catalog application are separate concerns. Each
implementation specification must provide an explicit SQLite lifecycle that:

1. validates the complete catalog before writes;
2. creates or migrates the SQLite registry schema through SQLx;
3. applies declarations by their natural identities;
4. updates only changed declarations; and
5. completes atomically or leaves no partial catalog application.

Reapplying unchanged declarations preserves stable registry identities and
avoids unnecessary record churn. Ordinary lifecycle operations must not delete
local state outside this canonical registry scope.

## Lifecycle and runtime behavior

Accepted SPEC-009 requires explicit database-state commands and states that
`serve` never applies database state implicitly. RFC-005 preserves that rule.

The focused implementation specifications define the SQLite command and
configuration interface. They require explicit preparation before Mother serves
a feature that depends on a migrated registry slice. `serve` may open and
validate the configured SQLite registry; it must not create it, migrate it, or
bootstrap it implicitly.

The implementation adds a dedicated SQLite schema and SQLx-backed repository
path. It must not claim PostgreSQL/SQLite interchangeability or degrade
PostgreSQL schema and query design solely for portability. Shared code is
appropriate only where behavior and SQLx abstractions are genuinely portable.

## Compatibility and transition

Migration proceeds by bounded registry slice, not by a database-wide cutover.
For each slice, a focused implementation specification identifies the
repository interface, every runtime read call site, the SQLite schema and
bootstrap declarations, and the PostgreSQL behavior that remains unmigrated.

At the point a slice becomes SQLite-owned, all of its runtime reads use the
SQLite repository in the same implementation change. No dual read or
PostgreSQL fallback is permitted. PostgreSQL registry writes may remain only
as temporary compatibility data for an unmigrated slice; no replication,
historical import, production cutover, or PostgreSQL schema deletion is
required by this RFC.

Existing PostgreSQL-backed behavior remains supported throughout the
transition. Its regression coverage remains required under the repository's
disposable PostgreSQL test path. Future domains move only through their own
accepted specification after ownership, lifecycle, and compatibility
requirements are established.

## Non-goals

This RFC does not:

- make SQLite a replacement for all PostgreSQL state;
- alter historical PostgreSQL migrations or require production-data import;
- add automatic migration or bootstrap during `mother-api serve`;
- add a local runtime profile, Docker-free environment, mock Bigwig boundary,
  demo authentication, module toggles, or ephemeral mode;
- add or change `/v1` endpoints, browser routes, OpenAPI operations, or
  `CONTRACTS.md` promises;
- implement DeFi position discovery or make unverified protocol data
  executable; or
- change the ownership of price availability, derivation, FX, historical price
  data, chain reads, or refresh scheduling.

Draft SPEC-024 continues to govern proposed DeFi position-discovery scope.
This RFC addresses only persistence for the canonical registry a future
accepted scope may use.

## Required follow-up specifications

This decision is implemented by two focused specifications:

1. [SPEC-033](../specs/SPEC-033-sqlite-canonical-asset-registry.md), covering
   the SQLite global-asset, network, and asset/network mapping registry and
   every current runtime reader of that catalog.
2. [SPEC-034](../specs/SPEC-034-sqlite-verified-protocol-registry.md), covering
   SQLite verified protocol registrations and targets, catalog declarations for
   them, and removal of PostgreSQL-only protocol seeding and runtime reads.

## Future implementation requirements

A slice implementation must demonstrate all of the following applicable
conditions:

1. A fresh SQLite registry can be explicitly created, migrated, and populated
   from the validated embedded catalog.
2. Repeating preparation is idempotent, leaves unchanged records stable, and
   rejects invalid declarations without partial application.
3. Canonical asset and `network_slug` resolution returns the expected native
   or deployed representation; an EIP-155 `chain_id` remains only the numeric
   property it represents.
4. Every runtime read for a migrated slice uses SQLite and fails explicitly
   rather than falling back to PostgreSQL.
5. The protocol slice, once implemented, resolves only declared,
   network-consistent verified targets through a compiled supported adapter.
6. Existing PostgreSQL-backed behavior and its disposable-database regression
   suite continue to pass.
7. The public API contract and OpenAPI coverage remain unchanged unless a
   separately accepted feature changes them and updates `CONTRACTS.md`.

## Consequences

This RFC establishes a deliberate incremental persistence direction, not an
implementation. It starts with the catalog Mother owns outright, then moves
verified protocol configuration after its dependency is available, and
preserves PostgreSQL for all other state. Acceptance authorizes the focused
implementation specifications; it does not itself authorize dependency,
schema, runtime, or public-contract changes.
