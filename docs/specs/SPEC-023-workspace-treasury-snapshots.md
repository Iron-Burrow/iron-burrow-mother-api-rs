---
status: accepted
owner: iron-burrow
last_reviewed: 2026-08-06
agent_edit_policy: update_when_relevant
---

# SPEC-023 - Workspace Treasury Snapshots

## Purpose

Add account-owned treasury summaries and manually requested immutable
Workspace snapshots using the established balance service and evidence model.

## Scope

- A snapshot selects 1-10 catalog asset slugs for active Workspace member
  addresses; the existing 100 member and 1,000 balance-resolution limits hold.
- Latest snapshots retain available Price Indexer valuations. Historical
  snapshots retain balance and Bigwig evidence but set valuation unavailable;
  Mother never substitutes a current price.
- Browser views and a private agent JSON export expose only the owning
  account's snapshots. Snapshots are append-only and never contain raw provider
  payloads or secrets.

## Non-goals

Scheduled refreshes, price derivation, P&L, DeFi positions, aggregation across
accounts, `/v1` routes, or historical price approximation.
