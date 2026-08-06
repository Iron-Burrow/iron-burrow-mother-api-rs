---
status: accepted
owner: iron-burrow
last_reviewed: 2026-08-06
agent_edit_policy: update_when_relevant
---

# SPEC-026 - Data Lab Workspace Scan

## Purpose

Replace the Scan holding page with account-authorized, Workspace-member Scan
tasks built from existing balance and ERC-20 transfer application services.

## Scope

Scan is limited to registered watch-only members, `eth-mainnet` and
`base-mainnet` balances, and `eth-mainnet` ERC-20 transfers. Transaction
lookup, generic explorer behavior, RPC, and arbitrary addresses remain out of
scope.
