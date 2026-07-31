---
status: accepted
owner: iron-burrow
last_reviewed: 2026-07-31
agent_edit_policy: update_when_relevant
---

# SPEC-017 - API-Key Account and Anonymous Ownership

## Purpose

Extend the private-Beta key model without changing its public machine API
contract: keys are legacy-consumer, `IBAccount`, or anonymous-demo owned.

## Scope

- Additive migration that backfills every existing key as `legacy`.
- Account grants as the account-key upper authorization boundary; operator CLI
  may issue account-owned keys only for active accounts.
- Anonymous demo issuance through `GET /access` plus `POST /access/demo`.
  Each one-time form intent mints one display-once key with both current read
  capabilities restricted to `eth-mainnet`, 24-hour expiry, 10/minute, and
  100/day policy.
- Caller resolution and canonical request-network grant checks for legacy,
  account, and anonymous callers.

## Non-goals

Public JSON key management, browser key management, billing, payments,
arbitrary RPC, organizations, and the deferred usage console.

## Security and operations

Raw keys are never stored or logged. Demo form responses are no-store and
no-referrer. Mother stores no visitor IP limiter; production must place an
external WAF ahead of Caddy with the accepted form limits (5 account entry
posts/15 minutes, 3 demo issues/day, 20 total form posts/day per source IP).
Tests prove ownership exclusivity, legacy compatibility, narrow demo grants,
expiry, duplicate-intent rejection, and no upstream call after a scope denial.
