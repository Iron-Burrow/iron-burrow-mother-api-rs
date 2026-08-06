---
status: accepted
owner: iron-burrow
last_reviewed: 2026-08-06
agent_edit_policy: update_when_relevant
---

# SPEC-019 - Client Registry and Delegated Access

## Purpose

Let an active IBAccount register a single-account Client (agent, script, or
dashboard) and issue account-bounded delegated credentials.

## Scope

- A Client has an opaque `ibc_*` public ID, active/revoked state, label, and
  exactly one active IBAccount owner.
- An `agent` key belongs to exactly one Client. Its authority is the
  intersection of account, Client, and key grants; revoking the account or
  Client immediately denies it.
- Browser Client management and private structured presenters are account
  owned. Raw keys remain display-once and are never logged.

## Non-goals

Organizations, shared Workspaces, a public key-management API, billing, or
delegation across accounts.

## Acceptance

Migration and authorization tests cover ownership exclusivity, revocation,
scope intersection, and the fact that legacy and pre-existing account keys do
not gain new capability grants.
