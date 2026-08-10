---
status: draft
owner: iron-burrow
last_reviewed: 2026-08-07
agent_edit_policy: update_when_relevant
---

# RFC-004 - Mother API Modular Product Runtime

## Status

Draft proposal. This RFC changes no runtime behavior, public route, OpenAPI
operation, or binding contract until it is accepted and implemented by focused
follow-up specifications.

## Summary

Here you have to discuss the fact of having a local deployment that works without the need to work with postgres or bigwig.

Mother API should remain one Rust binary and one repository. The Axum HTTP
adapters, Askama browser presentation, application services, persistence, and
bounded infrastructure adapters belong to that one modular runtime; they do
not communicate by calling Mother's own HTTP endpoints.

The runtime must also support a self-contained local product experience. Once
a native local PostgreSQL database has received Mother migrations and reference
data, a developer should be able to run:

```bash
cargo run
```

and open `http://localhost:3000` without a `.env` file, Docker, Caddy, Bigwig,
Price Indexer, or DIS. Local mode uses deterministic in-process mocks at each
external boundary. Production continues to use Bigwig for controlled chain
reads and Price Indexer for price and FX data.

## Current repository context

Mother already runs as one Axum binary and serves Askama pages from the same
runtime as `/v1`. `PUBLIC_API_SURFACE` currently selects Alpha or Beta behavior
only for the existing `/v1` route surface. It is not a general product-profile
mechanism. Current application services hold concrete Bigwig, Price Indexer,
and DIS clients; a general provider abstraction and a fixture-backed runtime
do not yet exist.

The accepted delivery policy is:

- [ADR-001](../adr/ADR-001-human-and-machine-domain-strategy.md) assigns human
  pages to `www.ironburrow.com` and the machine API to
  `api.ironburrow.com`; `/app` is not a delivery surface.
- [CONTRACTS.md](../../CONTRACTS.md) defines the binding `/v1` and documented
  human-web surface. Caddy applies this hostname split in production only.
- [RFC-003](<RFC-003 - Mother API El Vasco Architecture.md>) remains
  the accepted account and capability direction, except for its superseded
  hostname and `/app` material.

## Decision requested

Adopt the following architecture direction.

1. Mother remains a single-binary modular product runtime. No separate Lab,
   workspace, API, or frontend service is introduced.
2. Stable `/v1` contracts, browser-product routes, and experimental Lab studies
   remain distinct delivery concerns. A browser capability never creates a
   `/v1` operation by implication.
3. Runtime composition happens at bootstrap. Application code receives the
   capabilities and bounded dependencies it needs; it does not inspect an
   environment profile throughout handlers and domain services.
4. The default development invocation is a local profile with deterministic
   external mocks and native PostgreSQL. Production infrastructure selection is
   explicit and cannot accidentally resolve to local mocks.
5. New adapter seams are use-case-specific application ports, introduced only
   when Mother needs both production and local implementations. This RFC does
   not prescribe a generic chain, RPC, or pricing-provider trait.

## Delivery surfaces and module boundaries

`/v1` remains the stable machine API governed by `CONTRACTS.md`, OpenAPI, and
its accepted implementation specifications. Browser pages are server-rendered
human product transports. `/lab/*` remains experimental, private where its
study requires it, and does not gain a public JSON or `/v1` contract through
this RFC.

HTTP handlers are adapters. Shared behavior belongs beneath them in Mother
application services:

```text
/v1 handler ──────────┐
                       v
                application service
                       ^
web/Lab handler ───────┘
```

Neither a web handler nor a Lab study may call `localhost` or another Mother
HTTP route as an internal service bus.

The RFC does not prescribe a repository-wide directory rename. Implementations
must preserve useful existing boundaries and make only the module moves needed
by the adopted follow-up specification.

## Runtime composition

The eventual bootstrap configuration has two independent decisions:

1. **Runtime profile** determines whether the process is local or production.
2. **Capabilities and bounded adapters** are resolved once by bootstrap from
   that profile and validated configuration.

`MOTHER_PROFILE`, if introduced by the implementing specification, uses
`local` and `production`. When it is absent and `APP_ENV` is not `production`,
Mother resolves to `local`; `cargo run` therefore needs no profile variable.
An `APP_ENV=production` deployment must resolve to `production` unless an
explicit incompatible `MOTHER_PROFILE` causes startup to fail. The existing
`PUBLIC_API_SURFACE` setting remains the sole selector for Alpha/Beta `/v1`
behavior and is not replaced by this profile decision.

Production route exposure continues to be controlled by the accepted host
delivery policy and Caddy configuration. Local development accesses the
single Mother listener directly at `http://localhost:3000`; Caddy is neither
started nor required.

## Local profile

### Preconditions and defaults

Local mode uses only non-secret defaults:

```text
HTTP host:      0.0.0.0
HTTP port:      3000
PostgreSQL URL: postgres://postgres:postgres@localhost:5432/ibdb
Fixture set:    local-default-v1
```

The developer is responsible for running native PostgreSQL and, before
starting Mother, applying the repository's embedded migrations and reference
data. `cargo run` does not create a database, apply migrations, seed fixture
accounts, or mutate reference data.

Startup must fail before serving requests when the configured/default database
cannot be reached, when the schema migrations are not current, or when the
required reference-data lifecycle has not completed. The error must identify
the failed prerequisite without exposing credentials.

Normal email/password, session, ownership, CSRF, and capability rules remain
in force locally. Rendering the frontend requires no synthetic account or
authorization bypass; developers may create ordinary local accounts in the
prepared database.

### Deterministic external mocks

The built-in `local-default-v1` fixture set supplies bounded, versioned
responses for every current external dependency Mother invokes:

| Boundary | Local behavior | Production authority |
| --- | --- | --- |
| Chain reads | In-process Bigwig-compatible mock | Bigwig |
| Prices and FX | In-process Price Indexer-compatible mock | Price Indexer Query Layer |
| DeFi intelligence | In-process DIS-compatible mock | DIS, where an accepted capability uses it |

Mocks make no outbound HTTP or RPC request. They only implement inputs and
outputs required by currently supported Mother application flows and their
deterministic fixtures; they are not a chain emulator, price indexer, DeFi
indexer, or new service. Their fixture data must state its schema and fixture
version, retain deterministic ordering and timestamps, and never be presented
as live or historical production evidence.

The mock boundary is intentionally separate from production ownership:

- Bigwig remains the only approved production blockchain-read boundary.
- Price Indexer remains the source for production price availability,
  derivation, historical observations, and FX.
- Mother remains the runtime, policy, application, and presentation owner.
- Direct RPC and arbitrary user-selected providers are out of scope.

## Implementation constraints

An implementing specification may introduce a narrow port only after defining
the exact Mother use case, input/output model, errors, production adapter, and
fixture/mock adapter. Examples include a balance snapshot reader or a bounded
historical price observation reader. A universal `ChainProvider` that mirrors
Bigwig's HTTP API, a generic RPC fallback, and a provider chosen per request
are prohibited.

Bootstrap must log a redacted resolved runtime description, including profile,
enabled browser/API/Lab capabilities, adapter kinds, and fixture version when
local. It must never log connection secrets, API keys, raw provider payloads,
or account credentials.

Local mocks must be impossible in production by configuration validation and
composition tests. Production must reject incomplete credentials or provider
configuration rather than silently selecting fixture data.

## Relationship to focused product work

This RFC supplies runtime composition principles only. It does not authorize
or expand any of the following scopes:

- [SPEC-024](../specs/SPEC-024-mother-owned-defi-position-discovery-and-search.md)
  remains draft DeFi discovery and protocol-adapter scope.
- [SPEC-027](../specs/SPEC-027-data-lab-curated-research.md) governs bounded
  curated Lab studies.
- [SPEC-029](../specs/SPEC-029-defi-protocol-realized-yield-lab-study.md)
  governs the Aave V3 realized-yield Lab study.
- [SPEC-030](../specs/SPEC-030-vault-strategy-and-safe-controlled-execution.md)
  remains draft, separate Python/vault execution scope.
- [SPEC-032](../specs/SPEC-032-portfolio-strategy-simulation-lab.md) remains
  draft, separate portfolio simulation scope.

Python is not a local provider and this RFC creates no Python process,
notebook platform, execution engine, scheduler, strategy implementation, or
new public route.

## Non-goals

This RFC does not propose:

- a separate frontend, Lab, workspace, mock, or fixture service;
- a public admin, explorer, account, key-management, price, or DeFi route;
- a new `/v1` operation, OpenAPI operation, or `CONTRACTS.md` promise;
- direct RPC, arbitrary RPC/price endpoints, or a complete blockchain emulator;
- Docker, Caddy, Bigwig, Price Indexer, or DIS as a local Mother prerequisite;
- automatic migrations, automatic reference-data application, or fixture
  account seeding at application startup; or
- a generic test framework in place of focused application and adapter tests.

## Follow-up implementation order

1. Specify and implement validated runtime-profile resolution, direct local
   PostgreSQL readiness checks, and capability-aware router composition while
   preserving the current `/v1` contract.
2. Extract the smallest application ports needed by actual current flows and
   add production and `local-default-v1` mock implementations.
3. Add fixture schema/versioning and deterministic local browser-flow tests.
4. Migrate individual Lab or product capabilities only when their accepted
   specification defines their evidence, authorization, and failure behavior.

Each step requires its own implementation specification and tests. No broad
module rewrite is required merely to match this RFC.

## Acceptance criteria for a future implementation

An implementation following this RFC is complete only when all of the
following are demonstrated:

- With an empty environment and a prepared native default PostgreSQL database,
  `cargo run` serves the browser product at `http://localhost:3000`.
- The local process starts no Caddy process and makes no outbound Bigwig, Price
  Indexer, DIS, RPC, or other provider request.
- Local fixtures produce repeatable results for their supported application and
  browser flows, including a recorded `local-default-v1` fixture version.
- Missing PostgreSQL, unapplied migrations, or absent reference data stops
  startup with a specific, safe error.
- Existing production Bigwig and Price Indexer integration tests, `/v1`
  contract tests, and host-routing behavior continue to pass.
- Composition tests prove that a production runtime cannot resolve a local
  mock adapter and that `PUBLIC_API_SURFACE` retains its Alpha/Beta semantics.

## Open implementation questions

The following details belong to the first follow-up implementation
specification, not to this decision RFC:

- the exact readiness query proving migration and reference-data completion;
- fixture file format and the smallest fixture coverage that supports each
  current application flow;
- the concrete application-port names and error types extracted from the
  current Bigwig, Price Indexer, and DIS clients; and
- the production configuration validation matrix beyond the local and
  production profile rules established here.
