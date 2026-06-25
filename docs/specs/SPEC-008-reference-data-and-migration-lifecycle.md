---
status: draft
owner: iron-burrow
last_reviewed: 2026-06-25
agent_edit_policy: update_when_relevant
---

# SPEC-008 - Reference Data and Migration Lifecycle

Draft implementation specification for separating Mother API schema migrations,
canonical reference data, and development-only fixtures.

This document proposes an operational and database lifecycle change. It does
not change the public HTTP surface in [CONTRACTS.md](../../CONTRACTS.md), add a
public endpoint, or move another Iron Burrow service's responsibilities into
Mother API.

## Purpose

Mother API currently uses SQLx migrations for both structural database changes
and catalog data. As a result, adding or correcting a supported asset, network,
or asset-to-network mapping requires a migration even when the schema is
unchanged.

This spec separates three lifecycles:

1. **Schema migrations** are append-only structural changes and one-time data
   transformations.
2. **Reference seeds** are idempotent, repeatable declarations of canonical
   Mother API catalog data required in every deployed environment.
3. **Development seeds** are opt-in local or test fixtures that are never
   required by a production deployment.

After this lifecycle is implemented, routine catalog changes should not create
new schema migrations.

## Repository alignment

This spec follows the ownership boundaries in [AGENTS.md](../../AGENTS.md):

- Mother API owns the canonical `mother_api.global_asset`,
  `mother_api.network`, and `mother_api.asset_chain_map` catalog.
- The price-indexer Query Layer owns price availability, derivation, and
  history.
- DIS owns protocol-specific DeFi intelligence.
- The read-model service owns refresh scheduling and hot caches.

The quote currencies accepted by current public endpoints are defined by
runtime validation and `CONTRACTS.md`; Mother API does not currently have a
database-backed currency catalog. Creating one is out of scope for this spec.

[SPEC-006](SPEC-006-network-scoped-balances-v1.md) models one canonical active
asset representation for an `(asset_slug, network_slug)` pair. Reference data
must preserve that invariant.

## Current state

The repository currently has migrations `0001` through `0005`:

- `0001_mother_api_global_assets.sql` creates the three Mother-owned catalog
  tables and their initial indexes.
- `0002_seed_demo_global_assets.sql` inserts assets, networks, native mappings,
  and deployed mappings.
- `0003_global_asset_slug_unique.sql` makes `global_asset.slug` unique across
  lifecycle states.
- `0004_seed_mxnb_global_asset.sql` adds another asset and mapping.
- `0005_canonical_evm_network_slugs.sql` performs a one-time canonical network
  slug migration.

Although `0002` and `0004` are named as demo seeds, their rows currently
participate in the production catalog used by `/v1/assets`, `/v1/resolve`, and
the balance endpoints. They are therefore historical mixed-purpose migrations,
not disposable development fixtures.

The application process does not run migrations or seeds at startup. Compose
uses a separate `db-migrate` job that runs `sqlx migrate run`. The current
runtime image copies `migrations/` and the SQLx CLI, but it does not contain a
reference-seed runner or separate seed files.

## Goals

- Keep applied migrations immutable and reproducible.
- Make the canonical catalog declarative and reviewable outside migration
  history.
- Allow supported assets, networks, and mappings to be added or corrected
  without a schema migration.
- Make repeated reference-seed runs converge without duplicate rows, changed
  row identities, or no-op `updated_at` churn.
- Fail before catalog writes when declared relationships or identity keys are
  invalid.
- Apply reference data explicitly during deployment, before a new application
  release is allowed to serve traffic.
- Keep development-only fixtures out of production deployment paths.

## Non-goals

- Rewriting, deleting, squashing, or editing migrations that may already have
  run in a shared environment.
- Automatically deleting or inactivating database rows merely because they are
  absent from a seed file.
- Adding a currency table or moving quote-currency policy out of the public
  contract and runtime validation.
- Adding or changing public endpoints.
- Running migrations or seeds inside the Mother API application process.
- Moving price, protocol intelligence, cache scheduling, auth, billing, or
  indexing responsibilities into Mother API.
- Defining a general catalog administration API.

## Decision

Once this spec is accepted and implemented:

- Schema changes and required one-time data transformations continue to use
  numbered SQLx migrations.
- Canonical rows required by Mother API use ordered files under
  `seeds/reference/`.
- Local and test-only fixtures use files under `seeds/dev/`.
- The deployment database job runs schema migrations first and reference seeds
  second.
- The application starts only after that database job succeeds.
- Existing migrations remain unchanged. Reference seeds converge the
  post-migration database to the current canonical catalog.

Reference seeds become the source-controlled desired state for catalog fields
they own. The database remains the applied runtime state, while migrations
remain the historical record of structural evolution.

## Schema migrations

Migrations may:

- create or alter schemas, tables, columns, types, indexes, and constraints;
- add database functions required by schema behavior;
- perform a one-time transformation required by a schema change;
- establish deterministic identity constraints needed by reference seeds.

Migrations must not be used for routine additions or corrections to canonical
assets, networks, aliases, or asset-network mappings after the reference-seed
path is active.

A catalog change still requires a migration when it cannot be represented
safely under the current schema.

## Reference seed scope

Reference seeds may manage:

- global asset identity, display fields, aliases, metadata, lifecycle status,
  and sort order;
- network identity, family, chain ID, CAIP-2 identifier, metadata, lifecycle
  status, and sort order;
- native and deployed asset-network mappings, including address, deployment
  block, decimals, token standard, metadata, lifecycle status, and sort order.

Reference seeds must not:

- create fake users, balances, positions, or provider observations;
- write price-indexer, DIS, Bigwig, or read-model-owned data;
- call external providers;
- delete catalog rows as part of ordinary reconciliation;
- infer that an omitted row should become inactive.

Retirement is explicit. A reviewed seed change may set a known row to an
inactive or deprecated lifecycle state, but it must identify that row by its
canonical key.

## Deterministic identity prerequisites

Repeatable desired-state writes require conflict keys that remain stable when a
row changes lifecycle state. The current active-only partial indexes do not
provide that guarantee for every catalog table.

Before reference seeds become authoritative, structural migrations must enforce
these identities:

| Catalog row | Canonical identity | Required invariant |
| ----------- | ------------------ | ------------------ |
| Global asset | normalized `slug` | Exactly one row across all lifecycle states. Already enforced by `0003`. |
| Network | normalized `slug` | Exactly one row across all lifecycle states. Add normalization and full-lifecycle uniqueness. |
| Asset-network mapping | `(asset_id, network_id)` | Exactly one canonical mapping row across all lifecycle states. |

The mapping identity deliberately follows the existing
`(asset_slug, network_slug) -> representation` model. A future need for
multiple canonical representations of the same global asset on one network
requires a separate accepted schema and contract review.

Before adding these constraints, the migration must validate existing data and
fail with a diagnostic if duplicates or non-normalized keys exist. It must not
silently choose a winner.

Existing representation-safety constraints, including one active native asset
per network and one active mapping per network/deployment address, remain in
force.

## Reference seed semantics

Each seed run must have the following behavior:

- Missing canonical rows are inserted.
- Existing rows are updated in place by their deterministic identity.
- Existing IDs and `created_at` values are preserved.
- Seed-owned values replace previous values, including arrays and JSON
  metadata; stale seed-owned metadata must not survive through an implicit
  merge.
- Rows not declared by the seed are unchanged.
- A lifecycle transition is applied only when the seed explicitly declares the
  target status.
- `updated_at` changes only when at least one seed-owned value changes.
- A second run with identical inputs produces no row changes.

Upserts must use the full-lifecycle identity constraints, not active-only
partial indexes. Updates should use a distinctness predicate so no-op conflict
handling does not churn `updated_at`.

Illustrative pattern:

```sql
insert into mother_api.global_asset (
  slug,
  symbol,
  name,
  asset_kind,
  category,
  canonical_path,
  aliases,
  metadata,
  status,
  sort_order,
  updated_at
)
values (...)
on conflict (slug) do update
set
  symbol = excluded.symbol,
  name = excluded.name,
  asset_kind = excluded.asset_kind,
  category = excluded.category,
  canonical_path = excluded.canonical_path,
  aliases = excluded.aliases,
  metadata = excluded.metadata,
  status = excluded.status,
  sort_order = excluded.sort_order,
  updated_at = now()
where (
  mother_api.global_asset.symbol,
  mother_api.global_asset.name,
  mother_api.global_asset.asset_kind,
  mother_api.global_asset.category,
  mother_api.global_asset.canonical_path,
  mother_api.global_asset.aliases,
  mother_api.global_asset.metadata,
  mother_api.global_asset.status,
  mother_api.global_asset.sort_order
) is distinct from (
  excluded.symbol,
  excluded.name,
  excluded.asset_kind,
  excluded.category,
  excluded.canonical_path,
  excluded.aliases,
  excluded.metadata,
  excluded.status,
  excluded.sort_order
);
```

Network and mapping upserts must follow the same no-op behavior.

## Validation and failure behavior

The reference-seed run must execute in one transaction and fail without
partial reference-data writes.

Validation must occur before dependent inserts or updates. At minimum, the
runner must reject:

- a duplicate canonical asset slug in the declared seed input;
- a duplicate canonical network slug in the declared seed input;
- more than one declared mapping for an asset/network pair;
- a mapping whose `asset_slug` does not resolve to exactly one asset;
- a mapping whose `network_slug` does not resolve to exactly one network;
- a non-native mapping without a deployment address;
- a native mapping with a deployment address;
- missing decimals where required, or decimals outside the runtime-supported
  `0..=255` range;
- active mapping conflicts with native-asset or deployment-address
  uniqueness;
- legacy network slugs superseded by migration `0005`;
- any post-run violation of the deterministic identity prerequisites.

Mapping DML must not rely on an inner join that silently drops unresolved seed
rows. The seed must explicitly prove that every declared mapping resolves
before writing mappings.

Validation errors must identify the offending canonical key without printing
database credentials or unrelated environment values.

## Proposed repository layout

```text
migrations/
  0001_mother_api_global_assets.sql
  ...
  NNNN_reference_seed_identity_constraints.sql

seeds/
  reference/
    001_global_assets.sql
    002_networks.sql
    003_asset_chain_maps.sql
    900_validate.sql
  dev/
    001_local_demo_overrides.sql

scripts/
  apply-database-state.sh
  apply-reference-seeds.sh
  apply-dev-seeds.sh
```

Numeric prefixes define execution order. Reference filenames describe owned
catalog data rather than release numbers.

`apply-reference-seeds.sh` must:

- run from any working directory by resolving the repository root;
- require `DATABASE_URL`;
- stop on the first shell or SQL error;
- invoke PostgreSQL with `ON_ERROR_STOP`;
- apply all ordered reference files in one transaction;
- return non-zero on validation or write failure.

`apply-database-state.sh` runs:

```text
sqlx migrate run
apply-reference-seeds.sh
```

The implementation may use `psql` for the seed transaction. If so, the
runtime/migration image must install the PostgreSQL client and copy the
reference seeds and required scripts. Development seed files must not be
required by, or automatically run from, the production image.

## Environment and deployment policy

| Environment | Schema migrations | Reference seeds | Development seeds |
| ----------- | ----------------- | --------------- | ----------------- |
| Local development | Required | Required | Explicit opt-in |
| CI | Required | Required, including second-run proof | Only in tests that opt in |
| Staging | Required | Required | Forbidden |
| Production | Required | Required | Forbidden |

The Compose `db-migrate` service and production deployment workflow must use
the combined database-state command. A release must not start or roll out the
new application image when that command fails.

Mother API itself must not reconcile reference data at process startup. This
keeps application replicas read-only with respect to deployment lifecycle and
avoids concurrent seed execution.

Rolling back an application image must not automatically apply an older seed
bundle. Catalog reversals are forward changes reviewed and applied from the
current repository state.

## Forward-only transition

Implementation must preserve the existing migration history:

1. Add structural migration(s) for the deterministic identity prerequisites.
2. Create reference seed files representing the desired catalog after all
   existing migrations have run.
3. Use canonical post-`0005` network slugs such as `base-mainnet`,
   `mantle-mainnet`, and `arbitrum-mainnet`; do not copy stale pre-migration
   slug values into reference seeds.
4. Keep `0002_seed_demo_global_assets.sql` and
   `0004_seed_mxnb_global_asset.sql` unchanged.
5. On a new database, run all migrations and then converge the resulting rows
   through reference seeds.
6. On an existing database, run only unapplied structural migrations and then
   the same reference seeds.

The initial reference seed must be reviewed as production catalog data. The
historical `demo_seed` metadata label must not be preserved, removed, or
reinterpreted accidentally; its replacement is an explicit part of that
review.

## CI requirements

CI must prove:

1. all migrations apply to an empty PostgreSQL database;
2. reference seeds apply after migrations;
3. applying the same reference seeds a second time succeeds;
4. row IDs, `created_at`, and `updated_at` remain unchanged on the no-op second
   run;
5. required canonical assets, networks, and mappings have expected values;
6. every declared mapping resolves exactly once;
7. invalid seed fixtures fail atomically and leave the pre-run catalog
   unchanged;
8. development seeds are not part of the production database-state command;
9. the production migration image contains everything needed to run the
   database-state command.

Tests that inspect catalog data must depend on reference seeds explicitly
rather than relying on incidental data inserted by a historical migration.

## Operational change process

Adding or correcting an asset, network, or mapping normally requires:

1. editing the relevant reference seed;
2. running migrations and reference seeds against a clean database;
3. running the reference seed a second time;
4. running catalog and application tests;
5. reviewing canonical identifiers, lifecycle status, decimals, addresses,
   deployment blocks, aliases, and metadata;
6. deploying through the database-state job before application rollout.

Deleting catalog rows is not the normal removal mechanism. Support removal
should use an explicit inactive or deprecated state unless a separately
reviewed data-retention decision requires deletion.

## Implementation sequence

### Slice 1 - Identity constraints

- Add and test network slug normalization and full-lifecycle uniqueness.
- Add and test one canonical mapping row per asset/network pair.
- Fail migration on ambiguous existing data.

### Slice 2 - Reference runner

- Add ordered reference seed directories and scripts.
- Make the reference run atomic and fail-fast.
- Package the runner and required SQL client in the migration image.
- Update local and production database jobs to use the combined command.

### Slice 3 - Catalog transition

- Build the initial canonical reference files from the effective post-`0005`
  database state.
- Correct unresolved or stale identifiers through reviewed reference data,
  without editing historical migrations.
- Keep development-only fixtures separate and opt-in.

### Slice 4 - CI enforcement

- Add clean-database, second-run, no-churn, invalid-input, and image-content
  coverage.
- Make the reference lifecycle a release gate.

## Acceptance criteria

This spec is implemented when:

- existing migrations remain unchanged and continue to apply from an empty
  database;
- deterministic full-lifecycle keys support repeatable updates for all three
  catalog tables;
- canonical reference data has a separate, ordered seed path;
- the reference seed run is atomic and fails on unresolved relationships;
- routine asset, network, and mapping changes no longer require a migration;
- identical second runs preserve row identity and timestamps;
- omitted rows remain unchanged and retirement is explicit;
- development seeds cannot run through the production database-state command;
- local, CI, staging, and production deployment paths apply migrations before
  reference seeds;
- application rollout is gated on successful reference-data application;
- no public API contract or Iron Burrow service ownership boundary changes.

Until those criteria are met, migrations remain the repository's only
implemented database initialization path and this draft is not operational
truth.
