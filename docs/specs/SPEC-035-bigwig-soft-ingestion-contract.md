---
status: draft
owner: iron-burrow
last_reviewed: 2026-08-10
agent_edit_policy: update_when_relevant
---

# SPEC-035 - Bigwig Soft Ingestion Contract

## Purpose

Define a private, durable, version-tolerant ingestion boundary through which
Bigwig publishes operational information to Mother API. Bigwig submits neutral
versioned JSON envelopes and receives a durable acceptance receipt. Mother
alone owns classification, preservation, interpretation, domain
materialization, reports, and notifications.

This is a draft implementation specification. It changes no runtime route,
public API, OpenAPI operation, or binding `CONTRACTS.md` promise until it is
accepted and implemented.

## Context and ownership

Bigwig is Iron Burrow's dirty-operations runtime. It may run scheduled jobs,
infrastructure operations, long-running extraction processes, validator
monitoring, periodic collection, report generation, and future workflows that
Mother does not yet understand.

Bigwig may retain temporary operational state in local SQLite, including job
executions, logs, intermediate artifacts, and pending deliveries. That state
is not the permanent Iron Burrow record and may be removed under Bigwig's own
TTL policy after Mother durably accepts a publication.

Mother owns `ibdb` and all permanent persistence following acceptance. Bigwig
must not write directly to `ibdb`, know Mother tables, depend on Mother domain
schemas, send Telegram messages, or infer downstream processing results from
acceptance.

This preserves the accepted Mother/Bigwig boundary: Bigwig remains the
infrastructure and controlled blockchain-read boundary; Mother remains the
application, policy, product, reporting, and notification boundary.

## Scope

- One Bigwig-only private HTTP ingestion route and its neutral JSON envelope.
- At-least-once Bigwig delivery with idempotent Mother acceptance.
- Mother-owned PostgreSQL inbox persistence, triage, processor dispatch,
  retry, quarantine, and source-to-materialization provenance.
- A dedicated Bigwig ingestion API-key capability.
- Raw-envelope retention and future-safe handling of unknown artifact types,
  versions, and payload fields.
- The Mother-side foundation through which future report and notification
  processors may operate.

## Non-goals

- A public `/v1` endpoint, OpenAPI operation, public SDK, or general external
  webhook product.
- Kafka, NATS, a message broker, distributed event sourcing, or another
  deployable service.
- Bigwig access to PostgreSQL, `ibdb`, or Mother application tables.
- A strict shared artifact registry, a Mother-specific Bigwig domain model, or
  synchronized Bigwig/Mother releases.
- Concrete Bigwig artifact schemas, report rendering, Telegram transport,
  notification policy, or Mother-owned large-object storage.
- Treating Bigwig logs as permanent first-class Mother domain data.

## Delivery boundary

Mother will expose the following route only on the private Mother--Bigwig
service network:

```http
POST /internal/v1/ingest/bigwig
Content-Type: application/json
Authorization: Bearer <Mother-issued Bigwig ingestion key>
```

The production Caddy public hosts must not proxy this route, and it must not
be reachable through public DNS. The Mother handler independently enforces its
body-size limit; it must not rely on public Caddy limits that do not apply to
the private route.

The route is an internal service contract, not a stable public machine API.
It does not appear in `/v1`, OpenAPI, README endpoint lists, or
`CONTRACTS.md` as part of this draft.

### Authentication and authorization

The first implementation reuses Mother's established high-entropy bearer
API-key storage, validation, revocation, and per-key policy mechanism. It
creates a dedicated `internal` Bigwig consumer/key and a new
`ingest.bigwig.write` capability. The key must have only that capability and
must not be a customer, browser, anonymous-demo, or general-purpose key.

Mother's outbound `INFRA_GATEWAY_TOKEN` authenticates Mother-to-Bigwig calls
and must never be reused in the opposite direction. Private-network placement
and this dedicated capability are the initial defense-in-depth model. A later
service-principal or mTLS model may replace or augment its transport identity
without changing the envelope.

## Envelope and acknowledgement

Bigwig publishes one immutable envelope per `publication_id`.

```json
{
  "publication_id": "0198f1af-2e6c-7a7b-9c23-86f3fa5371d0",
  "process_id": "0198f1a8-10e8-70f4-a6e7-88d9f36d40e8",
  "artifact_type": "near.validator.snapshot",
  "artifact_version": 1,
  "produced_at": "2026-08-10T12:00:00Z",
  "payload": {
    "validator_id": "example.poolv1.near"
  }
}
```

The required fields are:

| Field | Rule | Meaning |
| --- | --- | --- |
| `publication_id` | UUIDv7 | Globally unique immutable submission identity and idempotency key. |
| `artifact_type` | Non-empty producer-defined string | General meaning of the artifact, such as `near.validator.snapshot`. |
| `artifact_version` | Positive integer | Producer schema/version label for the artifact type. |
| `produced_at` | RFC 3339 timestamp with explicit offset | Time at which Bigwig produced the artifact. |
| `payload` | Any JSON value | Producer-owned artifact content. |

`process_id` is optional. When supplied, it is UUIDv7 and identifies the
durable Bigwig execution/process that produced the publication. One process
may publish many artifacts. The first version intentionally does not define
separate `event_id` or `artifact_id` identities: `publication_id` identifies
the immutable delivery, while `process_id` provides operational correlation.

Mother derives the source identity from the authenticated credential; a
producer-controlled `source` field is neither required nor trusted. Additional
top-level fields and all unknown payload fields are accepted and preserved.

### Durable acceptance

Mother has accepted a publication only after one PostgreSQL transaction has
committed all of the following:

1. the authenticated-source receipt and acceptance timestamp;
2. the complete request bytes and SHA-256 digest;
3. the parsed envelope JSON and indexed envelope metadata; and
4. the initial triage/processing state.

Only then does Mother return:

```http
202 Accepted
```

```json
{
  "accepted": true,
  "publication_id": "0198f1af-2e6c-7a7b-9c23-86f3fa5371d0",
  "accepted_at": "2026-08-10T12:00:01Z"
}
```

The receipt means Mother now durably owns the envelope. It does not mean that
Mother recognizes it, has run a processor, has materialized a domain record,
has generated a report, or has sent a notification.

### Idempotency and delivery errors

Bigwig uses at-least-once delivery. It retains the exact request bytes and
retries whenever it cannot prove receipt of the `202` response. Mother stores
`publication_id` as a globally unique identity.

- A repeat request with the same `publication_id` and identical request bytes
  returns the original `202` receipt, including its original `accepted_at`.
- A request that reuses a `publication_id` with different bytes returns
  `409 idempotency_conflict`. It is an operator-visible producer/delivery
  fault and must not overwrite the accepted record.
- Network failures, timeouts, `429`, and `5xx` leave acceptance uncertain;
  Bigwig retries using its local delivery policy.
- `400`, `401`, `403`, `409`, `413`, and `415` do not mean accepted. Bigwig
  treats them as terminal until configuration, credentials, or the produced
  envelope is corrected.

Mother returns a client error before acceptance only for invalid JSON or media
type, oversize bodies, invalid required envelope primitives, non-UUIDv7 IDs,
invalid timestamp/version, failed authentication/authorization, quota denial,
or conflicting ID reuse. Mother does not reject an otherwise valid envelope
because its artifact type, version, or payload semantics are unknown.

## Mother inbox and processing model

All mutable ingestion state belongs in the existing `mother_api` PostgreSQL
schema. A separate `mother_ingest` or `mother_private` schema would add a
second naming/persistence convention without a security benefit; route and
database access controls provide privacy.

The implementation adds these private Mother tables:

- `mother_api.ingestion_publication` retains receipt identity, trusted source,
  indexed envelope fields, exact raw body, parsed JSON, digest, classification,
  processing state, retry lease, and retention timestamps.
- `mother_api.ingestion_attempt` is an append-only triage/processing history
  with timestamps, processor identity/version, and redacted failure metadata.
- `mother_api.ingestion_materialization` records the idempotent provenance
  from a source publication to each Mother-owned domain record it creates.

The inbox record's acceptance fields are immutable. Operational classification,
lease, retry, and retention fields may evolve; attempts preserve the audit
history of those transitions.

### Classification is distinct from processing

Every committed inbox row is accepted. Mother then records both a
classification and processing outcome:

| Classification | Meaning |
| --- | --- |
| `pending` | Awaiting worker triage. |
| `processable` | A registered processor supports the artifact type/version. |
| `unknown_type` | No processor is registered for the artifact type. |
| `unsupported_version` | The type is known but the published version is not supported. |
| `quarantined` | A known processor determined that the accepted data is semantically invalid or unsafe to materialize. |

| Processing state | Meaning |
| --- | --- |
| `pending` | Awaiting or eligible for a processor. |
| `leased` | Claimed by one worker under a bounded lease. |
| `succeeded` | A processor completed its idempotent materialization. |
| `retryable_failure` | The processor failed and is scheduled for retry. |
| `terminal_failure` | Retry has ended; the record remains retained and manually requeueable. |
| `not_applicable` | No current processor applies to the retained artifact. |

Unknown types and unsupported versions are retained rather than failed. A
later Mother release may register a processor and reclassify retained records.
Known artifact semantic validation happens after durable acceptance. A
semantic violation becomes quarantine, not a reason to make Bigwig lose its
only copy.

### Worker and recovery

The first implementation includes a bounded background worker in the same
Mother binary. It claims inbox rows transactionally using a lease, records the
attempt, and persists retry timing. Expired leases are reclaimable after a
crash. No external queue or second service is introduced.

Processor writes must be idempotent by `publication_id`. A successful domain
materialization and its provenance record must commit atomically. This lets
the worker retry safely after failures at the boundary between processing and
state recording.

This specification establishes the dispatcher and triage worker, but does not
introduce a concrete artifact-specific processor. Each future processor must
be introduced by focused accepted scope defining its artifact schema,
semantic validation, materialization, retention, and downstream effects.

## Raw preservation, logs, and attachments

Mother stores both the exact UTF-8 request body and parsed JSON. `jsonb`
preserves JSON meaning and supports queryability, but not object key order,
whitespace, or exact bytes; the raw body is required for forensic replay.
The request digest identifies the accepted bytes.

The default raw-content retention is 180 days. After that period, terminal
processed records may have raw bytes and parsed envelope content scrubbed
while their compact acceptance receipt, digest, classification, attempts, and
materialization provenance remain. Unknown, unsupported-version, and
quarantined records are not automatically scrubbed: an operator must first
make an explicit retention decision.

Bigwig may include selected logs, error context, metrics, report text, or
references in a payload. It must never include credentials, bearer tokens,
private keys, or other secrets. Raw payloads are not emitted into Mother logs.

The private route's hard body limit is initially 1 MiB. Large attachments are
not a v1 inline transfer mechanism. A referenced URL is retained only as a
reference; acceptance does not promise that Mother fetched or durably
preserved the referenced target. If Bigwig requires durable transfer of a
large object, a later Mother-owned upload/object-storage capability is
required.

## Reports and notifications

Bigwig may publish report artifacts such as `near.validator.report` without
knowing whether Mother will store, render, include, notify, ignore, or
quarantine them. A future report processor may materialize a report into a
Mother-owned immutable, versioned record linked to its source publication.
Superseding report content is a new publication, not an in-place change to a
previous materialization.

A future Iron Burrow daily activity report selects eligible Mother
materializations for its reporting period and creates its own immutable report
revision. It may include Bigwig-derived report material together with other
Mother-owned information.

Telegram and every other external notification remain Mother-only effects.
A processor or report service may create a Mother notification-outbox/delivery
record after successful materialization; that record owns deduplication,
attempts, provider response identifiers, and delivery status. Bigwig is
finished after durable acceptance and never delivers the notification itself.

## Observability and operations

Mother records structured, redacted observability for receipt, duplicate
replay, conflict, classification, attempt, lease recovery, retry, quarantine,
retention scrub, materialization, and notification-outbox transitions.

Operational views and metrics must make it possible to determine inbox depth,
oldest pending age, artifact type/version volume, duplicate/conflict rate,
retry count, terminal failures, quarantine volume, and raw-content retention
status. They must never expose raw bodies, payload values, API keys,
Authorization headers, or other credentials.

Private operator tooling may inspect, requeue, or apply an explicit retention
decision to a publication. No public administration route is introduced.

## Public-contract boundary

This specification deliberately does not expand the stable `/v1` contract.
The private route is not a customer/agent integration surface, does not appear
in generated OpenAPI, and does not require a `CONTRACTS.md` change for this
draft. Any future decision to expose ingestion beyond the private service
network requires separate accepted scope and a coordinated public contract
decision.

## Testing and acceptance criteria

An implementation is complete only when it demonstrates all of the following:

1. Mother commits the durable inbox row before returning `202` and returns no
   successful receipt when the database transaction fails.
2. The dedicated Bigwig key/capability is required, isolated from existing
   customer and browser capabilities, and its secret is not logged.
3. Exact duplicate bytes replay the original acceptance receipt; conflicting
   bytes for the same `publication_id` return `409` without mutation.
4. Unknown artifact types, unsupported versions, extra envelope fields, and
   extra payload fields are durably accepted and classified without a
   processor.
5. Structural validation, content type, UUIDv7 checks, body limit, and all
   pre-acceptance error classes behave as specified.
6. Worker leases recover after an interrupted worker; retryable and terminal
   processor failures retain a complete redacted attempt history.
7. Processor materialization is idempotent by source publication, including
   retry after a failure around the persistence boundary.
8. Raw request bytes remain available for the configured retention period and
   retention scrub preserves compact provenance without deleting materialized
   Mother data.
9. Existing `/v1`, browser, OpenAPI, and public Caddy route behavior remain
   unchanged.
10. `cargo fmt`, `cargo test`, `make test-db-postgres`,
    `make smoke-db-migrate`, and `git diff --check` pass.

## Dependencies and follow-up

This specification relies on the accepted Mother API capability and PostgreSQL
ownership foundations. It does not require Bigwig to expose a shared Mother
domain library. The corresponding Bigwig specification need only define how
Bigwig creates UUIDv7 publication/process identities, persists pending
deliveries locally, submits the neutral envelope, retries uncertain delivery,
and applies its own TTL only after Mother acceptance.

Future focused specifications may add artifact processors, report models,
daily-report selection, notification delivery, large-object ingestion, mTLS,
or additional private producers. None may make Bigwig depend on Mother domain
tables or downstream interpretation.
