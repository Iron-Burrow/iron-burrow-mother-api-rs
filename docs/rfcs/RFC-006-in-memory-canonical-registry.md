---
status: accepted
owner: iron-burrow
last_reviewed: 2026-08-07
agent_edit_policy: update_when_relevant
---

# RFC-006 - In-Memory Canonical Registry

## Status

Accepted architectural direction. This RFC supersedes RFC-005's SQLite
registry direction. It changes no runtime behavior, database lifecycle,
public route, OpenAPI operation, or binding contract until a focused
implementation specification is accepted and implemented.

## Summary

Mother API should operate its bounded, application-owned canonical knowledge
from an immutable `CanonicalRegistry` in process memory. The registry is built
once at startup from a versioned, reviewable catalog embedded in the release
artifact. Runtime canonical resolution performs neither PostgreSQL nor network
I/O.

PostgreSQL remains Mother's only persistence store. It continues to own mutable
application state; this RFC does not introduce SQLite, a local registry file,
or a second database lifecycle.

## Motivation

The current embedded `reference-data/catalog.json` already declares supported
assets, networks, and asset/network mappings. Mother validates that catalog but
then writes it to PostgreSQL and reads it back through
`GlobalAssetRepository`. Those reads are used for asset list/detail and search,
balance target resolution, ERC-20 contract metadata, and Workspace flows that
resolve canonical token information. Verified Aave configuration is likewise
seeded into PostgreSQL and queried at runtime.

This information is bounded and deterministic application knowledge, rather
than mutable runtime state. A separate SQLite store would duplicate the
catalog, add a new dependency and lifecycle, and still require a startup-time
read before Mother can serve. Loading the validated declarations directly into
memory removes this unnecessary persistence and lets canonical operations work
without PostgreSQL availability.

## Decision

Adopt an in-memory `CanonicalRegistry` as the sole runtime authority for
canonical application knowledge.

The registry is immutable after successful construction. Its declarations are
compiled into the release artifact, parsed and validated before the HTTP
listener is bound, and indexed for deterministic lookup. A malformed,
incomplete, ambiguous, or internally inconsistent catalog is a startup
failure. Runtime readers must not fall back to PostgreSQL, SQLite, a network
service, or a dynamically discovered value for canonical resolution.

PostgreSQL remains the only persistent datastore for Mother. This decision does
not make the process stateless: it only removes database reads whose sole
purpose is resolving application-owned canonical facts.

## Canonical registry boundary

The `CanonicalRegistry` contains only bounded declarations that are reviewed
and released with Mother:

| Data | Canonical identity | Required use |
| --- | --- | --- |
| Global asset | normalized asset slug | Name, symbol, aliases, category, canonical path, status, and ordering. |
| Network | `network_slug` | Supported-network name, family, status, CAIP-2 data, ordering, and EIP-155 `chain_id` when applicable. |
| Asset/network mapping | asset slug + `network_slug` | Native or deployed representation, token standard, decimals, canonical address, deployment block, status, and ordering. |
| Protocol registration | normalized protocol slug + `network_slug` | A compiled-adapter registration, versioned configuration, and verified/enabled state. |
| Protocol target | protocol identity + stable target key | Verified pool or reserve target, canonical address, and asset/network mapping reference when applicable. |

`network_slug` remains the public canonical supported-network identity. A
numeric `chain_id` remains distinct and appears only where it means an EIP-155
chain ID; it is not a generic replacement for `network_slug`.

The catalog must be extended with explicit protocol registrations and targets.
This replaces the PostgreSQL-specific hard-coded Aave V3 seed. A registry entry
does not make a protocol executable: compiled Mother adapters remain
responsible for ABI, calldata, decoding, and supported behavior.

Capability definitions may remain compiled domain knowledge and their grants,
policies, revocation, expiry, and request usage remain PostgreSQL state. A
registry must validate an otherwise static `network_slug`; it must not decide
whether a particular account, client, or API key has authority to use it.

## PostgreSQL boundary

The following data remains in PostgreSQL and is expressly outside the registry:

- accounts, identities, passwords, browser sessions, and anonymous issuance
  intents;
- API keys, capability grants, policies, revocation, expiry, and quota/usage
  counters;
- Workspaces, member addresses, labels, activity events, and treasury
  snapshots;
- portfolio simulation runs and their captured evidence; and
- all other user-controlled, historical, observed, discovered, cached, or
  otherwise mutable runtime data.

Price availability, derivation, FX, and historical price observations remain
the read-only responsibility of Price Indexer. Blockchain reads remain behind
Bigwig. Neither external response becomes a `CanonicalRegistry` record.

Existing PostgreSQL catalog tables and historical migrations are not deleted or
rewritten by this RFC. After implementation, they must not be a runtime source
for the registry. A focused specification must define the safe narrowing of the
PostgreSQL reference-data operation, including any remaining capability rows
and compatibility constraints, without making catalog replication a serving
prerequisite.

## Runtime behavior

Startup constructs the registry from the embedded catalog and performs all
cross-reference validation before building application state. It must validate
at least unique normalized identities, aliases, network references, mapping
uniqueness, native/deployed representation consistency, address format,
decimals, EIP-155 data, protocol-target references, and uniqueness of verified
enabled targets.

The registry exposes narrowly scoped, synchronous lookup operations. Its
implementation may precompute indexes such as:

- asset slug and normalized asset alias to global asset;
- `network_slug` to supported network;
- `(asset_slug, network_slug)` to asset/network mapping;
- `(network_slug, normalized contract address)` to canonical ERC-20 metadata;
  and
- protocol slug and target key to verified protocol configuration.

The catalog-dependent runtime paths must use those lookups: asset listing and
detail, asset resolution, balance target resolution, ERC-20 transfer token
resolution, Data Lab catalog views, Workspace balance/transfer metadata, and
the verified-protocol reader used by the existing Lab studies. A lookup result
must retain current deterministic ordering and public error semantics.

An unauthenticated catalog-dependent operation must be able to resolve its
canonical data with no database configured. Protected operations may still
query PostgreSQL for authentication, authorization, quota enforcement, or
user-owned state; that is separate from canonical resolution.

## Lifecycle and compatibility

The catalog is release data, not database state. `mother-api serve` loads and
validates it locally; it neither creates nor migrates a database nor writes
reference data. This retains the no-implicit-database-lifecycle rule from
SPEC-009 while removing a database preparation requirement for registry reads.

No `MOTHER_REGISTRY_SQLITE_URL`, SQLite migration set, registry file, or
`mother-api registry` lifecycle command is introduced. Mother retains its
existing PostgreSQL lifecycle commands for PostgreSQL schema and mutable-state
requirements.

The implementation replaces every runtime PostgreSQL catalog read in one
coherent change. It must not dual-read, fall back, compare at request time, or
replicate a runtime lookup into PostgreSQL. Postgres-backed regression tests
continue to cover the mutable-state repositories under the disposable test
database procedure.

## Relationship to RFC-005 and follow-up specifications

This RFC replaces
[RFC-005](RFC-005-incremental-mother-owned-sqlite-persistence.md). RFC-005 is
historical SQLite-direction material and is archived with this RFC as its
successor; it does not authorize SQLite work.

This decision is implemented in two ordered draft specifications:

1. [SPEC-033](../specs/SPEC-033-in-memory-canonical-asset-registry.md)
   establishes the immutable asset, network, and asset/network-mapping
   registry and migrates all readers for that slice.
2. [SPEC-034](../specs/SPEC-034-in-memory-verified-protocol-registry.md)
   extends the same registry with verified protocol declarations and migrates
   the existing Aave Lab readers after SPEC-033 is implemented.

Neither specification authorizes a SQLite dependency, schema, configuration,
or lifecycle command.

## Non-goals

This RFC does not:

- replace PostgreSQL for mutable Mother state or remove existing historical
  PostgreSQL migrations;
- introduce SQLite, any other local persistence store, a registry daemon, or a
  second deployable service;
- make user-controlled data, discovered contracts, prices, balances, caches,
  historical observations, or chain responses canonical;
- add automatic PostgreSQL migrations or reference-data writes at startup;
- add or change a `/v1` endpoint, browser route, OpenAPI operation, or
  `CONTRACTS.md` promise;
- authorize DeFi position discovery, arbitrary protocol configuration, direct
  RPC, or unverified protocol targets; or
- change the ownership of Price Indexer, Bigwig, or Read Model.

Draft SPEC-024 continues to govern proposed DeFi position-discovery scope. The
registry only supplies verified static configuration that an independently
accepted feature may use.

## Required follow-up specifications

Together, the implementation specifications must define:

1. the `CanonicalRegistry` domain model and its immutable lookup interface;
2. the catalog schema extension for verified protocol registrations and
   targets, plus complete validation and deterministic error behavior;
3. each current PostgreSQL catalog reader to replace, including the assets,
   balances, transfer-search, Workspace, and Aave Lab call paths;
4. the PostgreSQL reference-data lifecycle narrowed to its still-required
   mutable-state and capability concerns;
5. the migration compatibility treatment for existing catalog tables, without
   revising historical migrations; and
6. tests demonstrating the expected catalog behavior without a PostgreSQL
   pool, alongside the existing disposable PostgreSQL regression suite.

## Acceptance criteria for a future implementation

A conforming implementation demonstrates all of the following:

1. The embedded catalog creates a complete immutable registry at startup with
   no database or network I/O, and invalid declarations stop startup before a
   listener is bound.
2. Assets, networks, mappings, aliases, ERC-20 addresses, protocol
   registrations, and verified targets resolve only from that registry with
   deterministic ordering.
3. All catalog runtime readers operate without querying PostgreSQL or SQLite;
   tests prove the relevant public and browser application flows can resolve
   canonical data when no PostgreSQL pool is present.
4. Protected routes retain their current PostgreSQL-backed authentication,
   authorization, quota, and mutable-state behavior without treating those
   reads as registry fallbacks.
5. The existing public API response contracts and OpenAPI output remain
   unchanged unless an independently accepted change updates `CONTRACTS.md`.
6. `cargo test`, `make test-db-postgres`, and `make smoke-db-migrate` continue
   to pass; no SQLite dependency, configuration, migration, or lifecycle
   command exists.

## Consequences

Acceptance establishes one clear model: canonical knowledge is reviewed and
shipped with Mother, loaded once into memory, and never fetched at runtime.
PostgreSQL remains the single persistent owner of mutable Mother state. This
reduces catalog runtime coupling without changing service boundaries or
authorizing a public-contract change.
