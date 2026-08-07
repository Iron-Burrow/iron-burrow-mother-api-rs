---
status: draft
owner: iron-burrow
last_reviewed: 2026-08-07
agent_edit_policy: update_when_relevant
---

# SPEC-034 - In-Memory Verified Protocol Registry

## Purpose

Complete RFC-006's canonical-registry boundary by moving verified protocol
registrations and targets from PostgreSQL-specific seed/read code into the
immutable release-embedded `CanonicalRegistry` established by SPEC-033.

The initial implementation preserves the existing Aave V3 realized-yield Lab
study configuration exactly: `aave-v3` on `eth-mainnet`, its verified pool,
and the USDC, USDT, DAI, and GHO reserve targets. A catalog declaration remains
bounded static configuration; it cannot introduce arbitrary chain calls or a
new executable adapter.

## Scope

This specification extends `reference-data/catalog.json` with explicit,
reviewable protocol registrations and targets; validates those declarations
against the canonical asset/network registry; and replaces the runtime
`DefiProtocolRepository` reader and the PostgreSQL-only
`seed_aave_v3_realized_yield_protocol` reconciliation.

It covers the Data Lab realized-yield and portfolio-simulation protocol readers
that currently use `mother_api.defi_protocol` and
`mother_api.defi_protocol_target`. Compiled adapters retain ownership of ABI,
calldata, decoding, and behavior. Bigwig remains the blockchain-read boundary.

## Preconditions

SPEC-033 must be accepted and implemented first. It provides the immutable
asset/network/mapping identities required to validate and materialize protocol
targets. This specification introduces no intermediate dual-read state: once
its runtime wiring lands, no production protocol lookup may query PostgreSQL.

## Non-goals

This specification does not:

- add a protocol, `/v1` endpoint, browser route, OpenAPI operation, or public
  compatibility promise;
- enable DeFi position discovery, protocol search, arbitrary calldata, direct
  RPC, or user-submitted protocol configuration;
- replace PostgreSQL for accounts, sessions, API keys, grants, quotas,
  Workspaces, simulations, evidence, or any other mutable state;
- delete historical PostgreSQL protocol tables, migrations, or old rows;
- introduce SQLite, a registry service, a database migration, or a registry
  lifecycle command; or
- alter ownership of Price Indexer, Bigwig, or Read Model.

## Catalog declarations and validation

The catalog gains `protocols` and `protocol_targets` declarations with a
version bump and an explicit migration-compatible parser. The final JSON field
names may follow the existing catalog style, but each registration must declare
at least its normalized protocol slug, `network_slug`, protocol family,
compiled adapter kind and version, `enabled`, and `verified` state. Each
target declares a protocol reference, stable target key, target kind, canonical
lowercase EVM address, enabled/verified state, and, for reserve targets, the
referenced `(asset_slug, network_slug)` mapping.

Construction rejects, before startup:

- duplicate normalized protocol slugs or target keys;
- unknown or inactive networks, assets, or asset/network mappings;
- a target whose mapping network differs from the protocol network;
- unsupported target kinds, blank keys, invalid addresses, or impossible
  pool/reserve binding shapes;
- an enabled/verified protocol without exactly one enabled/verified `pool`
  target;
- enabled/verified reserve targets that are ambiguous by asset or address;
- registrations whose adapter kind/version is not supported by compiled
  Mother code; and
- the current Aave study's absence of one of its four declared reserve assets.

Disabled or unverified declarations remain non-executable catalog data. The
registry lookup for a study returns only an enabled, verified configuration
with a compiled adapter; it does not dynamically select adapters or invoke any
unreviewed target.

## Runtime interface and compatibility

The `CanonicalRegistry` adds a synchronous protocol lookup that returns the
domain information now represented by `RealizedYieldProtocol` and its ordered
reserves: protocol slug, `network_slug`, EIP-155 `chain_id`, adapter kind and
version, pool address, and reserve asset metadata/address. Its result ordering
must remain lexical by asset slug unless the existing route behavior proves a
different observable order.

`AppState` exposes this registry to the Data Lab and portfolio-simulation
application services independent of `database_pool`. Those services retain
their authentication, capability, account, Workspace, simulation persistence,
and Bigwig dependencies; only verified static protocol resolution moves.

`apply_embedded_catalog` stops seeding Aave protocol records and stops
reconciling protocol declarations into PostgreSQL. PostgreSQL protocol tables,
migration `0009_defi_protocol_registry.sql`, and existing deployed rows remain
historical compatibility material. They are neither deleted nor read at
runtime. No historical migration is revised.

## Implementation PR breakdown

### PR 1 - Declarative protocol catalog and validation

- Add typed protocol and target declarations to the embedded catalog and bump
  its version deliberately.
- Represent the existing Aave V3 pool and four reserve declarations exactly as
  the current PostgreSQL seed does.
- Extend catalog validation with protocol identities, target references,
  enabled/verified invariants, address normalization, mapping-network checks,
  and compiled adapter compatibility checks.
- Add unit tests for the valid Aave configuration and every invalid relation;
  no runtime reader changes in this PR.

### PR 2 - Immutable protocol indexes and domain projection

- Extend `CanonicalRegistry` with immutable protocol/target indexes and a
  narrow synchronous projection to `RealizedYieldProtocol`.
- Make projection ordering deterministic and reject ambiguity at construction,
  rather than resolving it at request time.
- Add no-database tests for protocol lookup, disabled/unverified filtering,
  stable reserve order, and startup failure for malformed declarations.

### PR 3 - Migrate Lab and simulation runtime readers

- Replace `DefiProtocolRepository` dependencies in the realized-yield and
  portfolio-simulation application services with the registry interface.
- Wire the registry through `AppState`, Data Lab presenters, and test
  fixtures, retaining existing capability and user-owned persistence behavior.
- Add route/service tests that execute canonical protocol resolution without a
  PostgreSQL pool and preserve current unavailable/unsupported study behavior.
- Remove the production PostgreSQL protocol read path; do not dual-read or
  compare records at request time.

### PR 4 - Narrow PostgreSQL reference-data ownership

- Delete the PostgreSQL-only Aave seed function and its invocation from the
  reference-data application path.
- Update disposable-PostgreSQL lifecycle tests to show `db apply` remains
  valid without seeding protocol rows and that historical tables need not be
  read to run the existing Lab study.
- Preserve migration `0009_defi_protocol_registry.sql` and deployed rows;
  this PR performs no destructive data cleanup.

### PR 5 - Regression audit and delivery verification

- Search production code for reads of `mother_api.defi_protocol` and
  `mother_api.defi_protocol_target`; remove or isolate any remaining runtime
  reader behind test-only historical coverage.
- Update `HISTORY.md` with the completed internal ownership move. Confirm no
  `CONTRACTS.md` or OpenAPI change is needed because no route or payload
  changes.
- Run the required verification suite and a focused Aave Lab smoke check using
  the configured Bigwig boundary where that environment is available.

## Testing and verification

Tests must distinguish static canonical resolution from mutable dependencies:
the registry works without a database; authentication, authorization, quota,
and simulation persistence retain their PostgreSQL tests. Tests must prove no
registry construction or protocol lookup performs network I/O.

Required implementation verification:

```sh
cargo fmt
cargo test
make test-db-postgres
make smoke-db-migrate
git diff --check
```

The focused Lab smoke is an additional integration check, not a substitute for
the disposable PostgreSQL suite. It must not be made a plain `cargo test`
requirement.

## Acceptance criteria

This specification is complete when:

- the embedded catalog contains the complete reviewed initial protocol and
  target declarations;
- invalid or ambiguous protocol configuration fails application startup before
  listener binding;
- the realized-yield and portfolio-simulation readers resolve verified static
  protocol data only from `CanonicalRegistry` without PostgreSQL, SQLite, or
  network I/O;
- compiled adapters and Bigwig remain the only executable/blockchain-read
  boundary;
- `db apply-reference` no longer seeds protocol configuration;
- historical migrations and data remain intact without runtime fallback; and
- all required verification passes without a public-contract change.

## Dependencies and follow-up

This specification completes RFC-006's present registry scope after SPEC-033.
Any future protocol, adapter, or DeFi position-discovery behavior requires its
own accepted scope; adding a catalog record alone never authorizes it.
