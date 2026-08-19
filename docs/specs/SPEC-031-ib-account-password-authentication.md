---
status: draft
owner: iron-burrow
last_reviewed: 2026-08-19
agent_edit_policy: update_when_relevant
---

# SPEC-031 - IBAccount Password Authentication and Browser Sessions

## Context

Mother already owns `ib_account`, `account_identity`, opaque hash-only browser
sessions, Askama web forms, `ib_account_capability_grant`, and authenticated
Data Lab routes. The implemented `SPEC-016` entry flow used Resend magic links
and email verification. This focused replacement makes the regular human-web
flow the conventional Mother-owned model:

```text
email + password -> ib_account -> browser session -> capabilities
```

It supersedes SPEC-016 when accepted. ADR-001 remains authoritative for the
`www.ironburrow.com` human host; the older RFC-003 `/app` topology is
historical. SPEC-013, SPEC-017, and migrations `0010`/`0013` remain the
authorization and account-ownership foundation.

## Goals and non-goals

- Allow any person to sign up, sign in, use authenticated Lab surfaces, and
  log out in the Mother API binary.
- Keep browser authentication separate from `/v1` bearer API keys and from
  anonymous demo keys.
- Do not add email verification, password recovery, magic-link login, OAuth,
  MFA, passkeys, organizations, billing, tiers, JWTs, or an identity service.

## Routes and flows

- `GET /signup` and `GET /login` render no-store Askama forms with only
  `email`, `password`, and a CSRF field. A signed-in browser redirects to
  `/lab`.
- `POST /signup` requires same-origin plus CSRF double-submit validation.
  Mother trims and ASCII-lowercases email, keeps the existing 254-character
  and basic-address validation, requires a 12–128-character password without
  composition rules, and returns a non-enumerating failure for duplicates.
  It creates an active `ib_account`, one active initial `Personal Workspace`,
  a pending/unverified identity, its baseline account and Lab capability
  grants, and a session in one transaction, then redirects to `/lab`. The
  initial Workspace does not restrict later Workspace creation or establish a
  preferred-Workspace setting.
- `POST /login` has the same origin/CSRF requirement. Missing, malformed,
  unknown, passwordless-legacy, disabled, suspended, closed, and wrong-password
  states all render the same `401` invalid-credentials response. Unknown and
  unusable identities perform a dummy Argon2id verification. A successful
  login rotates the account session and redirects to `/lab`.
- `POST /logout` keeps the existing same-origin and hash-verified CSRF checks,
  revokes the presented server-side session, clears both cookies, and redirects
  to `/`.
- Unauthenticated protected Lab and Workspace pages keep the current bare
  redirect to `/login`; authenticated callers without the required account
  grant receive `403` before Lab work.

## Data and security

- Add nullable `account_identity.password_hash`. New signups always write an
  Argon2id PHC string; existing rows stay nullable during migration. Argon2id
  uses a per-password random salt and at least 19 MiB memory, two iterations,
  and one lane. Successful logins may upgrade weaker stored hashes.
- A signup creates `ib_account.status = active` immediately. Its identity
  remains unverified (`verified_at = NULL`); verification is neither proof nor
  an authorization condition.
- Retain the `browser_session` 256-bit opaque token, SHA-256 hash at rest,
  eight-hour absolute expiry, 30-minute rolling idle expiry, `last_seen_at`,
  revocation, and active-account enforcement. Disabled identities also fail
  session resolution.
- `__Host-ib_session` is `Path=/; Secure; HttpOnly; SameSite=Lax`; the
  separate `__Host-ib_csrf` token is `Secure; SameSite=Lax`, not HttpOnly, and
  only its server-side hash is stored once a session exists. Entry forms use
  the same cookie as a same-origin double-submit token before session creation.
- Never log or render passwords, password hashes, raw session tokens, CSRF
  tokens, raw API keys, or link secrets. Private HTML stays no-store and uses
  the established CSP/Askama escaping policy.

## Authorization, compatibility, and operations

- Signup reuses existing `ACCOUNT_BASELINE` and `DATALAB_BROWSER_BASELINE`
  grants, including `lab.read`; it creates no customer level, RBAC model, or
  API-key grant.
- `/v1` remains bearer-key authenticated. Anonymous demo keys stay accountless;
  browser cookies never authenticate machine API calls and API keys never
  authenticate Lab HTML.
- The former `/verify-email` route and Resend account-entry configuration are
  removed. Existing passwordless records are retained with a null hash. A
  temporary operator-led password-migration process must be completed before
  a production cutover for any account that needs continuing access; it is not
  a browser login, signup, recovery, or email-verification feature.
- Production keeps the documented external WAF rule of five combined
  signup/login submissions per source IP per 15 minutes. Mother does not add
  a visitor-IP store or reuse the API-key limiter.

## Acceptance criteria

- Unit tests cover password bounds, Argon2id verification and rehash decision.
- Postgres tests cover immediate active-account and initial-Workspace creation,
  uniqueness, grants, session rotation/expiry/revocation, account suspension,
  and disabled identities.
- HTTP tests cover no-store form rendering, exact cookie flags, CSRF and
  same-origin rejection, generic login failure, `/lab` redirect, and removal
  of `/verify-email`.
- `cargo test`, `make test-db-postgres`, and `make smoke-db-migrate` pass;
  `/v1` API-key/OpenAPI and anonymous-demo behavior remain unchanged.
