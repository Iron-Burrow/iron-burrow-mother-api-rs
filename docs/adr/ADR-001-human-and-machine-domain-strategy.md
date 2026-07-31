---
status: accepted
owner: iron-burrow
last_reviewed: 2026-07-31
agent_edit_policy: update_when_relevant
---

# ADR-001 - Human and Machine Domain Strategy

## Decision

Iron Burrow separates public URLs by consumer, not by Mother API repository or
runtime ownership:

- `https://www.ironburrow.com` is the human web product. It serves `/`,
  `/scan`, `/scan/{network_slug}`, `/access`, `/docs`, and `/assets/*`.
- `https://api.ironburrow.com` is the stable machine API. It serves `/v1/*`,
  `/health`, and `/openapi.json`.

Both hostname surfaces run through the same Mother API Axum runtime. Caddy is
the public boundary and forwards unchanged paths only when they belong to the
hostname's allowlist; cross-surface paths return `404`.

`/app` and `/app/assets/*` are retired. `app.ironburrow.com` is reserved but
not configured. It may be introduced later only when a clearly separate
authenticated account/workspace product requires it.

## Consequences

Human-readable product and API documentation belongs at
`www.ironburrow.com/docs`; the generated machine contract remains at
`api.ironburrow.com/openapi.json`. No Swagger UI is currently implemented.

Local development continues to call the single Mother listener directly. The
documentation page obtains its OpenAPI link from `PUBLIC_API_BASE_URL`, whose
default is `http://localhost:3000`; production sets it to
`https://api.ironburrow.com`.

This ADR supersedes RFC-003's hostname and `/app` route-namespace decisions.
It does not change existing `/v1/*` compatibility promises or create a second
runtime, listener, or service.
