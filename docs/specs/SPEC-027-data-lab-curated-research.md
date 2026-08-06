---
status: accepted
owner: iron-burrow
last_reviewed: 2026-08-06
agent_edit_policy: update_when_relevant
---

# SPEC-027 - Data Lab Curated Research

## Purpose

Provide a bounded `/lab` catalogue of read-only studies over existing asset,
price, balance, transfer, and separately accepted protocol application services.

## Scope

Research identifiers and parameters are server-defined and validated. Studies
that address account data must target an owned Workspace member; accepted
protocol studies may instead use a verified protocol registry. HTML and
private JSON presenters share the same service path; no SQL, DSL, arbitrary
RPC, or public API route is introduced.
