---
status: active
owner: iron-burrow
last_reviewed: 2026-06-30
agent_edit_policy: update_when_relevant
---

# AGENTS.md

This repository, `iron-burrow-mother-api-rs` (Mother API), is part of the
Iron Burrow system. It is the Rust replacement for the old TypeScript
`iron-burrow-api` gateway and is currently in Production Alpha 1. Automated
agents and human contributors must protect code clarity, service boundaries,
API contracts, and project memory while working here.

## Required reading before editing markdown

Before editing or creating any markdown document in this repo, read
[DOCS.md](DOCS.md). It defines the documentation policy, status vocabulary,
front matter, and rules for where each kind of document belongs.

## Repository roles

Agents must respect the role of each document in this repo:

- [README.md](README.md) is the entrance and short navigation guide. It
  currently also doubles as the de facto endpoint contract reference.
- [DOCS.md](DOCS.md) is the documentation policy for this repo.
- [HISTORY.md](HISTORY.md) is the append-style project change log.
- `CONTRACTS.md` is the treaty: the promised public and internal contract
  surface for Mother API. It is **overdue** and must be authored, because
  Mother API already exposes reliable `/v1/*` endpoints documented in
  [README.md](README.md).
- [docs/rfcs/](docs/rfcs/) contains proposals and design discussions. RFCs are
  not current truth unless explicitly accepted.
- [docs/specs/](docs/specs/) contains accepted or draft implementation specs
  that follow accepted RFCs.
- [docs/adr/](docs/adr/) contains accepted architectural decision records.
- [docs/archive/](docs/archive/) contains historical memory.

## Agent rules

- Read [DOCS.md](DOCS.md) before touching markdown.
- Treat front matter `status` as authoritative. Location is a hint, not a
  promise.
- Make small, focused changes. Avoid unnecessary rewrites of existing docs or
  code.
- Do not modernize archived documents. Archived material is memory, not a
  draft to be improved.
- If a change adds or modifies an active endpoint, update or create
  `CONTRACTS.md` in the same change.
- Do not allow code behavior to imply promises that documentation does not
  explain.
- Do not invent endpoints, contracts, or guarantees that are not implemented
  and documented.
- Public API contracts must use `network_slug` for canonical supported network
  identity. Do not expose or accept a generic `chain` field; keep numeric EVM
  `chain_id` distinct when it truly means an EIP-155 chain ID.
- Do not rely on plain `cargo test` for Postgres-backed tests; those tests
  intentionally skip when `DATABASE_URL` is unset. Use
  `make test-db-postgres` to run Rust Postgres-backed regression tests against
  a disposable Docker Postgres database. Use `make smoke-db-migrate` to smoke
  test embedded migrations through the Mother API CLI / Docker image. Do not
  add tests that make plain `cargo test` run migrations or mutate arbitrary
  `DATABASE_URL` targets.
- Prefer additive edits to existing documents. When in doubt, add a new
  RFC, spec, or ADR rather than rewriting an existing one.
- Preserve service boundaries described in [README.md](README.md) ("Not
  Ported" section) and reinforced across the Iron Burrow system:
  - Mother API owns the public HTTPS surface, the canonical
    `mother_api.global_asset` / `network` / `asset_chain_map` catalog, and
    `/v1/search-engine`.
  - `iron-burrow-price-indexer` Query Layer owns price availability,
    derivation, and historical price data. Mother API consumes it read-only.
  - `iron-burrow-defi-intelligence-service` (DIS) owns protocol-specific
    DeFi intelligence, such as Aave V3 realized yield resolution. Mother
    API consumes DIS internal endpoints and must not reimplement protocol
    math, Bigwig archive calls, or reserve lookup.
  - `iron-burrow-read-model` owns refresh scheduling and hot caches.
  - Mother API does not own price indexing, event or holder indexing, auth,
    API keys, billing, x402 boundaries, admin/explorer/account/tracking
    routes, or in-process response caching.

## Scope guard

Mother API is in Production Alpha 1. Agents may make small, focused changes
consistent with the documented `/v1/*` contract in [README.md](README.md).

Do not reintroduce the old TypeScript gateway sprawl: no admin, explorer,
account, tracked-token, or price routes; no API-key context middleware; no
rate limiting; no in-process response caching; no auth, billing, or x402
boundaries.

New external dependencies, new public endpoints, or behavior changes that
expand Mother API's responsibilities require an accepted RFC or spec under
[docs/](docs/) before implementation.
