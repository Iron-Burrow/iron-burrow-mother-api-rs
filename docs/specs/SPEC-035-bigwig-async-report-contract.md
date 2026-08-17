---
status: accepted
owner: iron-burrow
last_reviewed: 2026-08-17
agent_edit_policy: update_when_relevant
---

# SPEC-035 - Bigwig Async Report Foundation

## Purpose

This accepted specification defines the feature-gated foundation for
account-owned asynchronous reports. It deliberately registers no report type
and leaves `ASYNC_REPORTS_ENABLED=false` in standard deployments. A future
accepted specification must add each concrete report type, its input and final
report schema, and its compatible Bigwig implementation before enabling the
feature.

## Scope and ownership

Mother owns the account-scoped report record, public report identifier,
idempotency boundary, terminal persistence, and account/agent authorization.
Bigwig owns execution and returns only a terminal completion or failure.
Neither service gains generic job execution, arbitrary report types, a public
SDK, notifications, or direct Bigwig access to Mother Postgres.

The feature-gated public operations are:

- `POST /v1/reports/{report_type}`, requiring an account or agent key with
  `reports.write` plus `Idempotency-Key`.
- `GET /v1/reports/{report_id}`, requiring an account or agent key with
  `reports.read`.

These routes are documented in `CONTRACTS.md` and OpenAPI only when the
Async Reports feature gate is enabled. An unsupported or unregistered type is
not a generic job request.

## Service credentials and routing

Mother calls Bigwig's report-execution route with the existing outbound
`INFRA_GATEWAY_TOKEN`. That credential is exclusively Mother-to-Bigwig and
must never authenticate a request to Mother.

Bigwig calls Mother only on the private service-network routes:

- `POST /internal/v1/reports/{report_id}/complete`
- `POST /internal/v1/reports/{report_id}/fail`

Both require `Authorization: Bearer <BIGWIG_REPORT_OUTCOME_TOKEN>`. This is a
distinct high-entropy Bigwig-to-Mother secret, not an API key or capability
grant. Missing, malformed, and incorrect credentials return the same `401
unauthorized` response before any handler or persistence access. Mother fails
startup when `ASYNC_REPORTS_ENABLED=true` without this token. Public Caddy
hosts do not proxy `/internal/v1/*`.

## Persistence, failure, and compatibility

Mother persists a report request before attempting Bigwig handoff. Reusing an
idempotency key with the same canonical request returns the original report;
reusing it with a different request returns `409 idempotency_conflict`.
Bigwig handoff that cannot be confirmed returns
`503 report_execution_unavailable` and a subsequent request with the same key
resumes the same resource.

Completion and failure records are terminal and immutable. The existing
completion boundary limits final report bodies to 1 MiB. Internal callback
authentication consumes neither customer API-key grants nor customer quota
usage. Existing `/v1` balance and transfer contracts are unchanged.

## Acceptance criteria

This foundation is complete when:

1. The embedded catalog and runtime capability registry contain exactly
   `reports.read` and `reports.write` for report access.
2. `reports.delivery.write` is absent from code, catalog, reference data, and
   public API-key authorization.
3. The feature gate defaults to disabled and configuration rejects an enabled
   report feature without `BIGWIG_REPORT_OUTCOME_TOKEN`.
4. Internal completion and failure callbacks accept only the dedicated inbound
   token, never `INFRA_GATEWAY_TOKEN` or a customer API key.
5. Public Caddy hosts do not proxy internal callbacks, credentials are redacted,
   and focused regression tests plus the required repository verification pass.

## Follow-up

Adding an enabled report type requires a focused accepted specification and
coordinated Bigwig contract. It must define the type/version, input, output,
delivery tests, rollout configuration, and any public contract additions.
