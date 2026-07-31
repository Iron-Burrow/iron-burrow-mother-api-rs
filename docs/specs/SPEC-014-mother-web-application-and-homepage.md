---
status: accepted
owner: iron-burrow
last_reviewed: 2026-07-31
agent_edit_policy: update_when_relevant
---

# SPEC-014 - Mother Web Application and Homepage

## Purpose

Add the first repository-owned public web surface for
`app.ironburrow.com` without creating a second service or duplicating Mother
API application logic.

## Scope

- Add Askama to the existing `mother-api` runtime and create public `/`,
  `/app`, and `/docs` HTML routes outside `/v1`.
- Add bounded static asset delivery only below `/app/assets/*` and
  deployment/Caddy configuration for the public host.
- Render links to public API documentation, Account entry points, demo-key
  onboarding, Workspace entry, Scan, and Lab as those routes become available.
- Add a public human-documentation route and a same-runtime
  `/docs/openapi.json` download that reflects the active feature-gated OpenAPI
  document.
- Establish HTML response headers, template conventions, and a session
  middleware seam; do not implement authenticated account behavior yet.
- Keep `/app` as an evolving application surface: Askama pages are first
  presenters, while structured agent-facing presenters use the same application
  services and authorization boundaries.

## Non-goals

- A separate frontend runtime, SPA, public API-key management, email login,
  anonymous key issuance, Scan/Lab implementation, payment, or new `/v1`
  endpoint.
- Public mention of internal development codenames.

## Dependencies

- RFC-003 and SPEC-013 for the authorization boundary.
- Existing Axum router and deployment topology.
- A dedicated documentation spec is required before any new public machine API
  documentation promise beyond current contracts; this SPEC may link to
  existing documentation only.

## Security relevance

Askama templates must use normal escaping and no unreviewed raw HTML. Public
pages set a reviewed CSP and safe content type; static asset paths are bounded
below `/app/assets/*`; no API key or session secret is embedded in HTML,
browser storage, logs, or analytics. Future state-changing forms require
SPEC-016 session/CSRF policy.

## Expected domain and database changes

None required for the initial homepage. The router may gain delivery-only view
models; templates must not import persistence adapters or make authorization
decisions. A session middleware interface may be added without a session table
until SPEC-016 is accepted.

## Expected public interfaces

- `GET /` returns an accessible, server-rendered product homepage.
- `GET /app` returns a public, non-functional Data Lab holding page. It does
  not authenticate callers, expose account data, or authorize capabilities.
- `GET /docs` returns human-readable current Beta API documentation.
- `GET /docs/openapi.json` returns the generated OpenAPI document for the
  active runtime configuration.
- `GET /app/assets/*` serves bounded static delivery; `/assets/*` is not a
  static delivery surface.
- All existing `/v1/*` and `/health` contracts remain unchanged.

No proposed Account, demo, Scan, or Lab path becomes a public promise in this
SPEC without an accepted dependent SPEC and matching `CONTRACTS.md` update.

## Acceptance criteria

- The existing binary serves the homepage and API without a second deployable.
- Homepage tests prove route registration, content type, no codename leak, and
  links to the currently documented API surface.
- Static delivery is limited to `/app/assets/*`, traversal is impossible, and
  cache policy is explicit.
- HTML responses use the reviewed CSP and security headers both directly from
  Mother and through Caddy; no inline scripts or styles are required.
- HTML templates do not bypass application authorization or expose secrets.
- Existing JSON route/OpenAPI compatibility tests remain green.

## Suggested implementation phase

Phase 2, after SPEC-013. Anonymous-key UI is explicitly deferred to SPEC-018;
verified Account UI is deferred to SPEC-016.
