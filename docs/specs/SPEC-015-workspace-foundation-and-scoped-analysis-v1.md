---
status: draft
owner: iron-burrow
last_reviewed: 2026-08-19
agent_edit_policy: update_when_relevant
---

# SPEC-015 - Workspace Foundation and Scoped Analysis v1

## Purpose

Define Workspace as the first-class durable product boundary for El Vasco.
Workspace is where account-owned watch-only context and analyses accumulate.
This spec establishes the minimum implementable Workspace model for the first
end-to-end Data Lab slice.

## Scope

- Workspace domain terminology and ownership invariants.
- Relationship between `IBAccount` and Workspace.
- Initial personal Workspace creation during IBAccount signup.
- Workspace lifecycle states and minimum fields.
- Watch-only address membership registration.
- Workspace labels.
- Account-owned Workspace lifecycle, member-address registration, and labels.
- Authorization boundaries for account-backed and anonymous callers.
- Shared application-service boundaries for human and agent presenters.

## Non-goals

- Project-management features (tasks, tickets, kanban, workflow automation).
- Full hypothesis notebooks, report builders, generated endpoint suites, or
  broad collaboration tooling in v1.
- Organization-shared Workspace semantics.
- New stable `/v1` public endpoint promises.
- Treasury advanced analytics beyond Workspace-scoped composition and evidence
  display.

## Dependencies

- RFC-003 for product model, surface policy, and promotion policy.
- SPEC-013 for authorization intersection and capability grants.
- SPEC-014 and ADR-001 for the `www.ironburrow.com` runtime shell and shared presentation boundaries.
- SPEC-012 and SPEC-007 for balance and transfer capability primitives used by
  Workspace-scoped views.

## Security relevance

Workspace must prevent cross-account data leakage and preserve evidence
integrity:

- A Workspace is always owned by exactly one `IBAccount` in v1.
- Anonymous callers cannot create or mutate Workspaces.
- Access checks must verify account ownership (or explicit delegated access in
  future specs) before any Workspace read/write operation.
- Watch-only membership must not imply custody or cryptographic control.

## Terminology

| Term | Meaning |
| --- | --- |
| Workspace | Durable account-owned boundary where Data Lab context and analyses accumulate. |
| Workspace member address | Watch-only on-chain address registered in a Workspace context. |
| Workspace label | User-defined organization tag associated with Workspace or member addresses. |

## Ownership and account relationship

- An `IBAccount` may own multiple Workspaces.
- A Workspace has exactly one owner `IBAccount` in v1.
- A newly created IBAccount receives one active initial Workspace named
  `Personal Workspace` in the same transaction as account creation. This is
  an onboarding convenience, not a preferred-Workspace field or a
  single-Workspace restriction.
- Workspace ownership transfer is deferred.
- Organization ownership and shared Workspace membership are deferred.

## Lifecycle

Minimum v1 lifecycle:

1. `active`: normal state for reads/writes.
2. `archived`: hidden from default list and read-only except for explicit
   restore/archive operations.

Deletion behavior in v1:

- Hard delete is out of scope.
- Archive is the supported retention boundary.

## Minimum fields (v1)

- `workspace_id` (stable opaque identifier).
- `owner_ib_account_id`.
- `name`.
- `description` (nullable).
- `status` (`active|archived`).
- `created_at`.
- `updated_at`.

## Watch-only membership and labels

Workspace member address v1 fields:

- `workspace_id`.
- `network_slug`.
- `address`.
- `client_ref` (nullable).
- `label_set` (zero or more labels).
- `created_at`.

Rules:

- Duplicate normalized `(workspace_id, network_slug, address)` entries are not
  allowed.
- Address registration is watch-only and does not assert ownership.
- Labels are user-defined metadata, not authorization primitives.

## Phase boundary

The append-only Workspace activity/evidence log, evidence-event persistence,
and an agent-facing structured equivalent are Phase 5 work owned by SPEC-021.
Phase 4 does not create activity-log tables or persist view events.

## Authorization boundaries

- Workspace operations require an authenticated account-backed principal in v1.
- Anonymous keys may call explicitly allowed non-Workspace capabilities but may
  not create/list/select/mutate Workspaces.
- Key-level grants can narrow Workspace-related operations and never expand
  beyond owner grants.

## Human and agent interfaces

Workspace capabilities are shared application operations exposed through
multiple presenters:

- Askama-rendered `www.ironburrow.com/workspaces` pages for human users.
- Structured application responses for clients/agents.
- CLI/future agent transports over the same service boundaries.

Interface/presenter differences must not change authorization semantics or
source-evidence semantics.

## Application service boundaries

Workspace route handlers and presenters must call Workspace application
services. They must not directly query infrastructure adapters or bypass
application authorization.

Canonical flow:

```text
Workspace application service
        ↓
Domain authorization and policy
        ↓
Balance/transfer/data adapters
        ↓
Presenter-specific rendering
```

## Persistence model (conceptual)

Conceptual tables and invariants:

- `workspace` (`workspace_id`, owner, status, metadata, timestamps).
- `workspace_member_address` (normalized workspace-network-address uniqueness).
- `workspace_label` and association tables.
Detailed schema decisions (indexes and retention tuning) are part of
implementation PRs and follow-on operational specs.

## MVP acceptance criteria

1. Account owner can create, list, rename, archive, and select Workspaces.
2. Account owner can register at least one watch-only address per Workspace.
3. Anonymous principals are denied Workspace operations.
4. Cross-account Workspace access is denied by ownership checks.
5. No new stable `/v1` endpoint is introduced by this spec.

## Deferred capabilities

- Workspace sharing across organizations or multiple account members.
- Workspace ownership transfer workflows.
- Rich hypothesis management, report publishing, and generated endpoints.
- Advanced treasury analytics and portfolio scoring.
- Automatic promotion of Workspace features to `/v1` without explicit readiness
  decision.

## Suggested implementation phase

Phase 4, after account-entry and caller-classification specs establish the
required principal model.
