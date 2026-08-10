---
status: draft
owner: iron-burrow
last_reviewed: 2026-08-10
agent_edit_policy: update_when_relevant
---

# SPEC-035 — Bigwig Async Report Contract

## Purpose

This draft defines the private, report-specific contract between Mother API and
Bigwig for one asynchronous report. It replaces the earlier generic
soft-ingestion draft: there is no artifact inbox, processor registry, worker
lease, notification dispatcher, or generic producer protocol.

Mother creates and owns the user-facing report request. Bigwig accepts that
request, performs the infrastructure work asynchronously, and returns only a
final report or terminal failure. This draft changes no implemented route,
public API, OpenAPI operation, or binding `CONTRACTS.md` promise.

It is coordinated with Bigwig draft SPEC-013, *Async Report Execution*.

## Scope and ownership

The initial report is:

```text
near.validator.daily.v1
```

Its v1 input is exactly:

```json
{}
```

Mother owns the UUIDv7 `report_id`, authorization, user/workspace
association, external report status, final report persistence, and retrieval
or presentation. A future user-facing report-start endpoint may call the
Mother report coordinator, but that public or browser surface is outside this
specification.

Bigwig owns the actual execution, process-local UUIDv7 `process_id`,
intermediate SQLite state, execution retries, progress checkpoints, and final
report construction. It never replaces `report_id` with `process_id`.

Neither service gains Telegram delivery, Bigwig access to PostgreSQL or
`ibdb`, large-object transfer, a public SDK, arbitrary report types, a
generic task API, or a general-purpose event bus.

## Private communication window

The two routes live only on the Mother--Bigwig service network. Public Caddy
hosts and public DNS must not proxy either route. They are internal service
contracts, not `/v1` endpoints, and are absent from OpenAPI, README endpoint
lists, and `CONTRACTS.md` until accepted and implemented.

### Mother starts Bigwig work

After Mother durably creates the report request and its `report_id`, its
report coordinator calls the Hub-only Bigwig route:

```http
POST /internal/v1/reports/start
Content-Type: application/json
Authorization: Bearer <mother-to-bigwig-report-token>
```

```json
{
  "report_id": "0198f1af-2e6c-7a7b-9c23-86f3fa5371d0",
  "report_type": "near.validator.daily.v1",
  "input": {}
}
```

Bigwig returns `202 Accepted` only after it has persisted the start:

```json
{
  "report_id": "0198f1af-2e6c-7a7b-9c23-86f3fa5371d0",
  "process_id": "0198f1b0-6c57-7b18-a99f-3e5a801e4b86"
}
```

Mother retains the original start body and retries it if it cannot prove that
Bigwig returned `202`. The same `report_id` with identical type and input
is an idempotent start replay; a different type or input for that ID is a
`409` conflict. The request body is limited to 256 KiB.

### Bigwig returns the final outcome

Bigwig calls Mother only after it has durably persisted a final report or
terminal execution failure:

```http
POST /internal/v1/reports/{report_id}/outcome
Content-Type: application/json
Authorization: Bearer <bigwig-to-mother-report-token>
```

The request body has one of these forms:

```json
{
  "kind": "completed",
  "process_id": "0198f1b0-6c57-7b18-a99f-3e5a801e4b86",
  "report_type": "near.validator.daily.v1",
  "report_version": 1,
  "produced_at": "2026-08-10T12:00:00Z",
  "report": {}
}
```

```json
{
  "kind": "failed",
  "process_id": "0198f1b0-6c57-7b18-a99f-3e5a801e4b86",
  "report_type": "near.validator.daily.v1",
  "failed_at": "2026-08-10T12:00:00Z",
  "failure_code": "report_output_too_large"
}
```

Mother validates the private credential, path `report_id`, UUIDv7
`process_id`, registered type, version, timestamp, body shape, and the
256-KiB body limit. It stores the final outcome atomically with the
Mother-owned report record before returning:

```http
202 Accepted
```

```json
{
  "accepted": true,
  "report_id": "0198f1af-2e6c-7a7b-9c23-86f3fa5371d0",
  "accepted_at": "2026-08-10T12:00:01Z"
}
```

A completion gives Mother durable final report data; it does not imply any
particular web rendering, notification, or additional interpretation. A
failure gives Mother durable terminal failure information; it does not cause
Bigwig to restart work.

For `near.validator.daily.v1` / version `1`, `report` is the exact
versioned JSON object defined in Bigwig SPEC-013: validator identity,
as-of/evidence timestamps, `current`/`stale`/`incomplete` evidence status,
nullable terminal-block and stake fields for incomplete evidence, and the
canonical decimal stake string. Mother stores that object without changing
its meaning. A breaking report shape requires a new report version and
focused accepted scope in both repositories.

## Mother persistence and idempotency

Mother persists a report request before attempting the Bigwig start call. It
retains the user/workspace association, report type/input, external state,
the accepted Bigwig process ID, and exactly one final outcome.

Mother calculates and stores a SHA-256 digest of the exact private outcome
request bytes. A repeated identical outcome for the same `report_id` returns
the original `202` receipt. A different outcome body, outcome kind, process
ID, type, or version for a report that already has an accepted outcome returns
`409 idempotency_conflict` without modifying the original result.

This report-specific idempotency is sufficient for the first version. There
is no separate `publication_id`, generic publication table, generic inbox,
or downstream processor state.

Mother accepts no outcome without a matching report request. It does not
accept unknown report types or versions in this first version, because Mother
itself initiated the only registered report type. Additional report types
require a focused accepted specification that updates both registered sides.

## Failure and retry rules

Mother-to-Bigwig start delivery and Bigwig-to-Mother outcome delivery use the
same small classification:

- timeout, connection failure, `408`, `429`, and `5xx` are uncertain or
  retryable; the caller retries its already-persisted exact request, using
  valid `Retry-After` when supplied;
- `400`, `401`, `403`, `409`, `413`, and `415` are terminal and
  operator-visible; they do not authorize a different report/result to reuse
  the same `report_id`.

Mother's report coordinator owns the external state for an unsuccessful start
attempt. Bigwig owns execution retries and sends failure only after execution
has irrecoverably terminated. Neither side uses a generic delivery worker or
distributed queue in this version.

## Limits, security, and observability

Both private routes independently enforce a 256-KiB body limit. A final report
that does not fit must be retained as a Bigwig local execution failure with
the stable `report_output_too_large` code; Bigwig sends only the small
failure outcome. URLs and attachments do not promise durable transfer or
preservation.

The two directions use distinct high-entropy bearer secrets:

- Mother-to-Bigwig report-start credential;
- Bigwig-to-Mother report-outcome credential.

They are not customer API keys, browser credentials, Edge credentials, or the
existing Mother-to-Bigwig gateway token. Configuration and logs redact them.

Mother logs and metrics may expose report ID, process ID, report type/version,
state, retry count, and redacted error class. They must not expose report
content, input, raw request bodies, Authorization headers, or credentials.

Mother owns final-report retention under its report-record policy. It owns
preservation after accepting a final outcome; Bigwig may apply its local
retention policy independently.

## Acceptance criteria

Implementation is complete only when tests demonstrate:

1. Mother persists its report request before the Bigwig start attempt.
2. The private start and outcome routes are isolated from public routing and
   require their distinct private credentials.
3. Bigwig start acknowledgment, duplicate replay, and conflicting start
   behavior match this specification.
4. Mother commits a final result/failure before returning `202`; identical
   outcome replay returns the original receipt and a changed outcome returns
   `409` without mutation.
5. All retryable and terminal status classes, UUIDv7 validation, type/input
   validation, body limits, and secret redaction are covered.
6. Existing public `/v1`, browser, OpenAPI, Bigwig Hub/Edge, and Mother
   gateway-call behavior remain unchanged.

## Relationship to other specifications

This document replaces the previous draft `SPEC-035-bigwig-soft-ingestion-contract.md`;
that draft has no accepted or implemented behavior to preserve.

Bigwig SPEC-013 defines Bigwig's durable execution and recovery behavior.
When it is accepted, it supersedes Bigwig SPEC-012; that transition is a
Bigwig documentation change, not a Mother API migration.

Future focused specifications may add one registered report type/version,
its input, working-state rules, final report schema, user-facing initiation,
or presentation. They must not widen this contract into generic ingestion or
workflow infrastructure.
