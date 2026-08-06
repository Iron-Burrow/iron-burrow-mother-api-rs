---
status: accepted
owner: iron-burrow
last_reviewed: 2026-08-06
agent_edit_policy: update_when_relevant
---

# SPEC-018 - Quotas, Allowances, and Usage

## Purpose

Add account and Client quota ceilings plus sanitized, append-only usage events
without changing the existing private-Beta per-key limits or introducing
billing, credits, or entitlements.

## Scope

- A policy may impose a daily request ceiling on an active IBAccount or its
  Client. The effective ceiling is the strictest configured account, Client,
  and existing key policy limit.
- Mother records a redacted usage event after each protected private product
  operation. Events contain opaque IDs, capability, network scope, outcome,
  HTTP response class, and timestamp; they never contain credentials, request
  bodies, labels, or provider responses.
- The quota decision happens before upstream work. Existing minute and daily
  per-key policies remain the compatibility baseline.

## Non-goals

Billing, plan entitlements, payment/402, public rate-limit headers, a public
usage API, or a response cache.

## Acceptance

Tests prove an account or Client ceiling cannot be bypassed by a narrower key,
denied and rate-limited work does not reach an upstream adapter, and usage
events never persist secrets.
