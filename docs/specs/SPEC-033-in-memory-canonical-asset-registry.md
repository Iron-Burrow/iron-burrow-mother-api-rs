---
status: draft
owner: iron-burrow
last_reviewed: 2026-08-07
agent_edit_policy: update_when_relevant
---

# SPEC-033 - In-Memory Canonical Asset Registry

## Purpose

Implement RFC-006's first registry slice: the immutable, release-embedded
canonical facts for global assets, networks, and asset/network mappings.
Mother must construct this registry before binding its HTTP listener and use it
for every runtime read in this slice, without PostgreSQL, SQLite, or network
I/O for canonical resolution.

## Scope

This specification covers the existing `reference-data/catalog.json`
declarations for assets, networks, and `asset_chain_maps`; their validation;
deterministic in-memory indexes; and all current readers of those records.
It replaces runtime use of `GlobalAssetRepository::Database` for this slice,
including asset listing/detail and resolution, balance catalog resolution,
ERC-20 transfer token resolution, Data Lab catalog views, and Workspace
balance, transfer, and search metadata.

The existing `GlobalAssetRepository::InMemory` test fixture is not the
production registry. The implementation may reuse verified mapping logic, but
it must build production data solely from the embedded catalog and must not
retain a runtime PostgreSQL variant as a fallback.

Verified protocol registration and target data are deliberately out of scope.
They remain PostgreSQL-backed until SPEC-034 is accepted and implemented. That
later change depends on this asset/network registry for protocol-target
cross-reference validation.

## Non-goals

This specification does not:

- add or change an HTTP route, OpenAPI operation, response payload, or
  `CONTRACTS.md` promise;
- add SQLite, a registry URL, a migration, or a registry CLI command;
- remove or rewrite historical PostgreSQL migrations or catalog tables;
- make `serve` run PostgreSQL migrations or write reference data;
- move capabilities, authentication, authorization, quotas, accounts,
  Workspaces, portfolio records, prices, blockchain responses, or caches into
  the registry; or
- implement protocol discovery, arbitrary token discovery, or DeFi positions.

## Required model and behavior

### Construction

`CanonicalRegistry` is a typed, immutable application value constructed from
the same catalog bytes embedded with `include_str!`. Construction parses and
validates the full catalog before `AppState` is available and before the HTTP
listener is bound. It returns a clear startup error for malformed JSON or any
invalid declaration; it must never produce a partially populated registry.

The catalog remains source-controlled release data. `mother-api serve` reads
it locally only; it neither connects to PostgreSQL for canonical data nor
creates, migrates, or reconciles any database state.

The parser may continue to validate capability declarations for the separate
PostgreSQL reference-data lifecycle. Asset/network construction must not
require a PostgreSQL pool, and capability rows must not be treated as registry
records.

### Identities and validation

The registry must preserve the current canonical identities:

| Record | Identity | Required validation |
| --- | --- | --- |
| Global asset | normalized asset slug | unique slug and aliases; valid asset fields and ordering |
| Network | `network_slug` | unique normalized slug; valid family, CAIP-2, and EIP-155 fields |
| Asset/network mapping | `(asset_slug, network_slug)` | known active/inactive asset and network references; representation, address, decimal, and deployment consistency |
| ERC-20 metadata | `(network_slug, normalized contract address)` | unique active EVM ERC-20 representation without ambiguity |

`network_slug` is the only canonical supported-network identity. `chain_id`
remains an optional numeric EIP-155 property; no interface may repurpose it as
a generic network selector.

Validation must reject at least duplicate normalized identities or aliases,
unknown references, duplicate mappings, duplicate canonical token addresses,
invalid EVM addresses, invalid decimals, invalid native/deployed combinations,
and inconsistent EVM `chain_id`/CAIP-2 declarations. It must preserve the
current handling of inactive records and the public meanings of unsupported
network, unavailable mapping, unavailable asset-on-network, and non-ERC-20
asset errors.

### Lookup interface and ordering

The registry exposes narrowly scoped synchronous lookups sufficient for the
current services. The exact Rust type names may be chosen in implementation,
but the interface must support:

- confident asset matching and bounded recommendations by normalized query;
- asset listing and detail by slug, including ordered network mappings;
- a network lookup by `network_slug`;
- ordered balance targets for requested assets on a network;
- canonical ERC-20 token metadata by network and normalized address; and
- a mapping lookup by asset slug and `network_slug`.

All lookups retain the current externally observable ordering: declared sort
order first and the existing deterministic lexical tie-breaker thereafter.
Lookup data is borrowed or cheaply shared from the immutable registry; no
request may query or mutate a catalog store.

### Application wiring and compatibility

`AppState::try_new` must create one shared registry independent of
`database_pool`. The state and application services receive that registry for
canonical lookups. Routes that also need PostgreSQL retain their existing
optional repositories for mutable state, authentication, authorization,
quota, or Workspace ownership only.

If no database is configured, unauthenticated catalog-dependent asset
operations must still resolve canonical records. Protected operations may fail
for their documented authentication or mutable-state dependency, but never
because they attempted a canonical PostgreSQL fallback.

The existing PostgreSQL catalog tables and historical migrations remain
untouched. `mother-api db apply-reference` is narrowed so it continues to
reconcile only declarations still required by PostgreSQL (currently capability
definitions and legacy-grant compatibility). It must stop writing global
assets, networks, or asset/network mappings. Existing rows remain historical
compatibility data and are not read by this slice.

## Implementation PR breakdown

### PR 1 - Typed catalog and deterministic validation

- Extract the asset, network, and mapping catalog declarations from the
  PostgreSQL reconciliation path into a typed canonical-registry module.
- Build `CanonicalRegistry` and its indexes from embedded catalog bytes.
- Add exhaustive unit tests for valid construction, normalized identity and
  alias collisions, invalid references, mapping invariants, EVM address and
  decimal validation, deterministic ordering, and indexed ERC-20 lookup.
- Keep all runtime wiring unchanged in this PR; it introduces no fallback or
  public behavior change.

### PR 2 - Startup ownership and reference-data narrowing

- Construct the registry in `AppState::try_new` before dependent application
  state and before server startup.
- Define a sanitized startup error path for an invalid embedded catalog; test
  that listener construction is not reached on failure.
- Remove global asset, network, and mapping reconciliation from
  `apply_embedded_catalog`, preserving capability reconciliation and the
  explicit database lifecycle required by SPEC-009.
- Add disposable-PostgreSQL tests showing `db apply` remains valid for fresh
  and existing databases without treating catalog rows as a serving
  prerequisite.

### PR 3 - Asset and resolution readers

- Replace asset listing, asset detail, confident match, recommendation, and
  `/v1` resolution service reads with the registry interface.
- Replace production test fixtures that depend on
  `GlobalAssetRepository::InMemory` with focused registry builders where that
  makes the dependency explicit.
- Add route and service tests that preserve payloads, error shapes, and
  ordering with `database_pool: None`.
- Remove the PostgreSQL asset repository from these runtime paths rather than
  retaining dual-read or request-time comparison code.

### PR 4 - Balance, transfer, Data Lab, and Workspace readers

- Move `CatalogBalanceTargetResolver`, ERC-20 transfer token lookup, Data Lab
  catalog presenters, and Workspace balance/transfer/search metadata to the
  registry.
- Keep Bigwig and Price Indexer calls, API-key checks, capability checks,
  Workspace ownership checks, and mutable-state reads unchanged and separate.
- Add focused no-database canonical-resolution tests plus protected-route
  tests proving PostgreSQL reads remain only for their documented mutable
  concerns.

### PR 5 - Remove obsolete production read paths and verify

- Remove or make test-only the PostgreSQL global-asset reader APIs after all
  production call sites have moved; do not alter historical tables or
  migrations.
- Audit with repository search that no production canonical asset/network/map
  lookup targets `mother_api.global_asset`, `mother_api.network`, or
  `mother_api.asset_chain_map`.
- Update `HISTORY.md` with the implemented internal ownership change. Confirm
  that `CONTRACTS.md`, OpenAPI, and endpoint examples require no edit because
  the public contract is unchanged.
- Run the full verification suite below.

## Testing and verification

Implementation must add characterization tests for current public payloads,
error codes, and ordering before replacing each reader. It must also test that
catalog construction and catalog-dependent services work with no PostgreSQL
pool, while malformed catalog fixtures fail before application startup.

Required verification for the completed implementation:

```sh
cargo fmt
cargo test
make test-db-postgres
make smoke-db-migrate
git diff --check
```

The PostgreSQL suite remains disposable and explicit; plain `cargo test` must
not migrate or mutate an arbitrary database target.

## Acceptance criteria

This specification is complete when:

- one immutable registry is constructed from the embedded catalog before the
  HTTP listener binds;
- every production asset/network/mapping reader named in scope uses it and
  performs no PostgreSQL, SQLite, or network I/O for canonical resolution;
- invalid catalog data fails startup deterministically without partial state;
- catalog-dependent paths preserve current ordering and documented errors;
- `db apply-reference` no longer makes asset/network/map writes necessary for
  serving canonical records;
- PostgreSQL remains unchanged as owner of mutable state; and
- the required verification commands pass with no public-contract change.

## Dependencies and follow-up

RFC-006 must remain accepted. SPEC-034 depends on this specification being
implemented because it uses the registry's asset/network mapping identities to
validate protocol targets.
