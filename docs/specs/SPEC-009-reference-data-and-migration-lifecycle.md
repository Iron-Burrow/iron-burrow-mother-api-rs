---
status: accepted
owner: iron-burrow
last_reviewed: 2026-07-01
agent_edit_policy: update_when_relevant
---

# SPEC-009 - Mother API Database State Lifecycle

Draft implementation specification for separating Mother API schema migrations
from required reference data.

This specification defines the database lifecycle Mother API should use going
forward. It replaces the previous mixed approach where schema migrations,
reference data, SQLx CLI usage, `psql`, and shell scripts were all part of the
database-state path.

The goal is a simpler model:

> The Mother API application image is the release artifact.
> The Mother API binary applies database state through explicit commands.
> `serve` never applies database state implicitly.

This spec does not change the public HTTP API surface.

---

## Important changes

This spec changes the database-state model going forward:

* future assets, networks, aliases, mappings, catalog metadata, statuses, and
  sort order changes must not be added through new migrations;
* existing mixed migrations remain historical records and must not be edited;
* `0004_seed_mxnb_global_asset.sql` is an example of the old mixed data
  migration pattern, not the model to copy;
* new catalog rows belong in the required reference-data source and are applied
  by `mother-api db apply-reference`;
* reference data must be idempotent, so rerunning the same declarations does not
  rewrite unchanged rows.

---

## Problem

Mother API currently has migrations `0001` through `0005`.

Some of those historical migrations mix structural schema changes with catalog
data that the application needs at runtime. They may also contain rows originally
called “seed” or “demo” data even though those rows now participate in the real
runtime catalog.

Those migrations must not be edited.

Going forward, Mother API needs a clean lifecycle:

1. schema migrations evolve database structure;
2. required reference data declares canonical runtime data;
3. deployment explicitly applies both before the application serves traffic.

The next database work, including the API-key subsystem, must use this model.

---

## Decision

Mother API will expose explicit database lifecycle commands:

```text
mother-api serve
mother-api db migrate
mother-api db apply-reference
mother-api db apply
```

Where:

```text
mother-api db apply = mother-api db migrate + mother-api db apply-reference
```

The production deployment path must run:

```text
mother-api db apply
```

before rolling out:

```text
mother-api serve
```

`mother-api serve` must not run migrations, apply reference data, reconcile
catalog rows, or perform deployment lifecycle work.

---

## Image policy

Mother API uses one production application image.

That image must contain everything needed to run:

```text
mother-api serve
mother-api db migrate
mother-api db apply-reference
mother-api db apply
```

The production database lifecycle must not depend on:

* `sqlx-cli`;
* `psql`;
* a separate migration image;
* a separate seed image;
* shell scripts as the source of truth.

Shell scripts are allowed only as thin convenience wrappers around the Mother API
binary. They must not contain the canonical migration or reference-data logic.

The same image version must be used for `db apply` and `serve`.

---

## Command semantics

### `mother-api db migrate`

Applies embedded SQLx migrations.

Requirements:

* use `DATABASE_URL`;
* run unapplied migrations in order;
* preserve SQLx migration history;
* fail non-zero on migration failure;
* not apply reference data;
* not start the HTTP server.

The implementation should embed migrations in the binary, for example through
SQLx embedded migrations or an equivalent compile-time mechanism.

### `mother-api db apply-reference`

Applies required reference data.

Requirements:

* use `DATABASE_URL`;
* run in a single database transaction;
* acquire a transaction-scoped PostgreSQL advisory lock;
* validate declared reference data before dependent writes;
* fail without partial writes;
* upsert reference rows idempotently;
* preserve existing row IDs;
* preserve `created_at`;
* update `updated_at` only when reference-owned values actually change;
* leave omitted rows unchanged;
* never infer deletion or retirement from omission;
* not run schema migrations;
* not start the HTTP server.

The advisory lock must use a stable Mother API key, for example:

```sql
select pg_advisory_xact_lock(
  hashtextextended('mother_api.reference_data', 0)
);
```

### `mother-api db apply`

Runs the full database-state operation:

```text
1. mother-api db migrate
2. mother-api db apply-reference
```

The command must fail non-zero if either step fails.

### `mother-api serve`

Starts the HTTP application only.

It must not apply migrations or reference data.

---

## Historical migration boundary

Existing migrations `0001` through `0005` remain unchanged.

They are historical records, even if some of them are mixed-purpose or poorly
named.

The new lifecycle begins after `0005`.

Forward-only transition:

1. keep `0001` through `0005` unchanged;
2. add new structural migrations only for schema changes or one-time structural
   data transformations;
3. add deterministic identity constraints needed by idempotent reference-data
   upserts;
4. move routine catalog additions and corrections into required reference data;
5. run all migrations first, then apply reference data.

On a new database:

```text
mother-api db apply
```

must produce the current valid runtime database state.

On an existing database:

```text
mother-api db apply
```

must apply only pending migrations and then converge required reference data.

---

## Schema migration rules

Schema migrations may:

* create or alter schemas, tables, columns, indexes, constraints, and database
  functions;
* create API-key subsystem tables and indexes;
* add deterministic identity constraints needed by reference data;
* perform one-time transformations required by a schema change.

Schema migrations may contain data only when the data change is a one-time
structural transformation required by the schema change itself.

Schema migrations must not be used for routine additions or corrections to:

* supported assets;
* supported networks;
* asset/network mappings;
* aliases;
* display metadata;
* sort order;
* lifecycle status;
* other small canonical catalogs that are required at runtime.

A migration that only adds or corrects a global asset, network, mapping, alias,
display metadata, lifecycle status, or sort order is invalid under this
lifecycle.

A catalog change requires a schema migration only when the current schema cannot
represent the desired state safely.

---

## Required reference data

Required reference data is canonical source-controlled data that Mother API needs
in every deployed environment.

Examples:

* supported assets;
* supported networks;
* asset/network mappings;
* required asset aliases;
* required catalog metadata;
* required lifecycle status;
* required API-key scopes, plans, or policy rows, if the API-key subsystem
  introduces such static catalogs.

Required reference data must not include:

* real issued API keys;
* plaintext API key secrets;
* hashed API key secrets;
* customer-specific records;
* request usage records;
* audit events;
* runtime observations;
* price-indexer-owned data;
* DIS-owned data;
* Bigwig-owned data;
* read-model-owned data.

Real API-key issuance is operational administration, not migration or reference
data.

---

## Reference data representation

The first implementation target is one source-controlled JSON file, for example:

```text
reference-data/catalog.json
```

The Mother API binary must embed that file, for example with `include_str!`, and
parse it with the existing `serde_json` dependency.

The reference-data file must declare assets, networks, and asset/network mappings
together so relationship validation can happen before writes.

The production reference-data path must not require:

* `serde_yaml` or another new parser dependency for the first implementation;
* `psql`;
* external SQL seed scripts;
* shell scripts containing canonical reference-data logic.

The implementation must satisfy these requirements:

* the Mother API binary applies the reference data;
* no external CLI is required in production;
* the data is reviewable in pull requests;
* validation happens before writes;
* writes are transactional;
* second identical runs do not churn timestamps;
* unresolved relationships fail loudly;
* omitted rows remain unchanged.

Do not introduce unnecessary public abstractions or premature generic frameworks.
Use the simplest representation that satisfies the lifecycle requirements.

---

## Deterministic identity requirements

Required reference data needs stable conflict keys.

Before reference data becomes authoritative, Mother API must enforce deterministic
identity constraints where needed.

Required catalog identities:

| Catalog row           | Canonical identity       |
| --------------------- | ------------------------ |
| Global asset          | normalized `slug`        |
| Network               | normalized `slug`        |
| Asset/network mapping | `(asset_id, network_id)` |

The desired reference-data identity for mappings is the resolved
`(asset_id, network_id)` pair from declared `asset_slug` and `network_slug`.
Deployed token uniqueness by `(network_id, lower(deployment_address))` remains a
database safety constraint, but routine reference-data reconciliation must key
declared mappings by canonical asset/network identity.

The structural migration that adds or verifies these constraints must:

* validate existing data first;
* fail if duplicate identities exist;
* fail if non-normalized keys exist;
* not silently choose a winner;
* preserve existing active-representation safety constraints.

The existing mapping model remains:

```text
(asset_slug, network_slug) -> one canonical representation
```

If Mother API later needs multiple canonical representations of the same asset on
the same network, that requires a separate accepted spec.

---

## Reference data semantics

Each reference-data run must behave as follows:

* insert missing declared rows;
* update existing declared rows in place;
* preserve row IDs;
* preserve `created_at`;
* change `updated_at` only when at least one reference-owned value changes;
* replace reference-owned arrays or metadata with the declared value;
* leave omitted rows unchanged;
* apply lifecycle transitions only when explicitly declared;
* use inactive or deprecated status for retirement;
* never delete rows as ordinary reconciliation;
* produce no row changes on a second identical run.

Reference data declares desired values for the fields it owns. It must not infer
state from absent rows.

Adding one new asset, network, or mapping to the JSON file must insert or update
only the newly declared or changed rows and must leave unchanged catalog rows
untouched.

---

## Validation requirements

`mother-api db apply-reference` must validate declarations before dependent
writes.

At minimum, it must reject:

* duplicate declared asset slugs;
* duplicate declared network slugs;
* duplicate declared asset/network mappings;
* non-normalized slugs;
* empty slugs;
* mappings whose asset slug does not resolve exactly once;
* mappings whose network slug does not resolve exactly once;
* native mappings with deployment addresses;
* deployed mappings without deployment addresses;
* invalid deployment address format;
* missing required decimals;
* decimals outside the supported range;
* legacy network slugs superseded by previous canonicalization migrations;
* post-run violations of deterministic identity constraints.

Validation errors must identify the offending canonical key.

Validation errors must not print credentials, API keys, secrets, or unrelated
environment values.

---

## Transaction and locking policy

`mother-api db apply-reference` must run in one transaction.

Inside that transaction it must acquire a transaction-scoped advisory lock before
validation and writes that depend on reference-data consistency.

If validation fails, the transaction must roll back.

If any write fails, the transaction must roll back.

Partial reference-data application is not acceptable.

---

## API-key subsystem boundary

The API-key subsystem should use this lifecycle as follows:

Schema migrations create the structural model:

* API-key tables;
* key hash columns;
* key prefix columns;
* status columns;
* expiration columns;
* indexes;
* constraints;
* audit or usage tables, if included in the accepted API-key spec.

Required reference data may create static policy rows if needed:

* default scopes;
* permission names;
* plan identifiers;
* static rate-limit policy identifiers.

Required reference data must not create real API keys.

If Mother API needs API-key issuance commands, they must be separate
administrative commands and not part of this database lifecycle spec.

---

## Deployment policy

Every environment must apply database state explicitly before serving the new
release.

Required production sequence:

```text
1. Build or pull Mother API image version X.
2. Run `mother-api db apply` using image version X.
3. If `db apply` succeeds, run `mother-api serve` using image version X.
4. If `db apply` fails, do not roll out `serve`.
```

The image version used for `db apply` must match the image version used for
`serve`.

Rollback must not automatically run `db apply` from an older image.

Database state is forward-only. Catalog reversals must be new forward reference
data changes. Schema reversals require reviewed forward migrations.

---

## CI requirements

CI must prove:

1. `mother-api db migrate` works from an empty PostgreSQL database;
2. `mother-api db apply-reference` works after migrations;
3. `mother-api db apply` works from an empty PostgreSQL database;
4. a second identical `db apply-reference` run succeeds;
5. a second identical run preserves row IDs;
6. a second identical run preserves `created_at`;
7. a second identical run preserves `updated_at`;
8. required canonical assets, networks, and mappings have expected values;
9. every declared mapping resolves exactly once;
10. invalid reference data fails atomically;
11. invalid reference data leaves the pre-run catalog unchanged;
12. the production image can run `mother-api db apply`;
13. the production image can run `mother-api serve`;
14. the production image does not require `sqlx-cli`;
15. the production image does not require `psql`;
16. adding one new asset to the JSON file inserts only that missing row and any
    newly declared related mappings;
17. invalid duplicate slugs, unresolved mappings, native mappings with
    deployment addresses, and deployed mappings without deployment addresses fail
    before writes;
18. production image commands do not require seed scripts or a separate
    migration image.

---

## Implementation sequence

### Slice 1 - Lifecycle commands

* Add `mother-api db migrate`.
* Add `mother-api db apply-reference`.
* Add `mother-api db apply`.
* Ensure `mother-api serve` does not run database lifecycle work.
* Ensure all database lifecycle commands use `DATABASE_URL`.

### Slice 2 - Embedded migrations

* Run SQLx migrations from the Mother API binary.
* Remove production dependency on `sqlx-cli`.
* Keep migrations `0001` through `0005` unchanged.
* Prove clean-database migration from the binary.

### Slice 3 - Identity constraints

* Add structural migration(s) after `0005` for deterministic reference-data
  identities.
* Validate existing data before adding constraints.
* Fail on duplicates or non-normalized keys.
* Preserve existing active-representation safety constraints.

### Slice 4 - Required reference data

* Add `reference-data/catalog.json` as the source-controlled required
  reference-data representation.
* Embed it in the Mother API binary and parse it with `serde_json`.
* Apply it from the Mother API binary.
* Validate declarations before writes.
* Run inside one transaction.
* Acquire the advisory lock.
* Use idempotent upserts.
* Prevent no-op `updated_at` churn.
* Ensure unresolved mappings fail loudly.
* Do not add `serde_yaml`, external SQL seed scripts, or production `psql`
  requirements for reference-data application.

### Slice 5 - API-key adoption

* Implement API-key schema through new migrations.
* Add static API-key policy reference data only if required.
* Do not create real API keys through migrations or reference data.

### Slice 6 - Deployment and CI hardening

* Use the same image for `db apply` and `serve`.
* Remove production need for `sqlx-cli`.
* Remove production need for `psql`.
* Gate rollout on successful `mother-api db apply`.
* Add CI checks for clean apply, second-run no-op behavior, invalid reference
  data, and production image capabilities.

---

## Acceptance criteria

This spec is implemented when:

* migrations `0001` through `0005` remain unchanged;
* schema migrations are applied by the Mother API binary;
* required reference data is applied by the Mother API binary;
* production does not require `sqlx-cli`;
* production does not require `psql`;
* production uses one image with multiple explicit commands;
* `mother-api serve` never applies migrations or reference data;
* `mother-api db apply` applies migrations before reference data;
* reference-data application is atomic and advisory-locked;
* reference data is declared in a source-controlled JSON catalog embedded in the
  Mother API binary;
* routine asset, network, and mapping changes no longer require schema
  migrations;
* reference-data upserts preserve IDs and `created_at`;
* identical second runs preserve `updated_at`;
* a second identical `db apply-reference` run produces no row changes;
* adding one new asset to the JSON file inserts only that missing row and any
  newly declared related mappings;
* invalid duplicate slugs, unresolved mappings, and bad deployment
  address/native combinations fail before writes;
* omitted rows remain unchanged;
* retirement is explicit;
* API-key tables are added through new schema migrations;
* real API keys are not created through migrations or reference data;
* local, CI, staging, and production use the same lifecycle model;
* application rollout is gated on successful `mother-api db apply`.

Until these criteria are met, this spec is not operational truth.
