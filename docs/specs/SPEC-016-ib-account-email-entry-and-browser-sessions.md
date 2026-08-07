---
status: superseded
owner: iron-burrow
last_reviewed: 2026-08-07
agent_edit_policy: update_when_relevant
superseded_by: docs/specs/SPEC-031-ib-account-password-authentication.md
---

# SPEC-016 - IBAccount Email Entry and Browser Sessions

> Superseded by [SPEC-031](SPEC-031-ib-account-password-authentication.md).
> This document records the former passwordless email-link implementation.

## Purpose

Implement RFC-003 Phase 3 verified `IBAccount` entry on the human web host.

## Scope

- `IBAccount`, verified email identity, one-time `signup`/`login` links, and
  passwordless sessions.
- `GET/POST /signup`, `GET/POST /login`, `GET/POST /verify-email`, and
  `POST /logout` on `www.ironburrow.com`.
- Hash-only token/session storage, 15-minute entry links, session rotation,
  8-hour absolute expiry, 30-minute idle expiry, and CSRF protection.
- Resend delivery using configured sender/origin values. Public submissions
  always return generic outcomes and never enumerate identities.

## Non-goals

Passwords, OAuth, recovery factors, organizations, Workspace UI, self-service
key management, `/v1` additions, and `/app` routes.

## Security and acceptance

The link GET is confirmation-only; only the same-origin POST consumes it.
Session identifiers are `Secure`, `HttpOnly`, `SameSite=Lax`, `__Host-`
cookies; CSRF tokens are separately hash-verified. Suspended or closed
accounts cannot authenticate. Tests cover expiry, one-time consumption,
generic responses, cookie properties, logout, and no secret logging.
