---
status: active
owner: iron-burrow
last_reviewed: 2026-07-30
agent_edit_policy: update_when_relevant
---

# SPEC-001 - DIS Protocol Intelligence Boundary

## Status and purpose

This active specification preserves an architectural boundary: the
`iron-burrow-defi-intelligence-service` (DIS) is the appropriate owner of
protocol-specific, read-only intelligence that Mother API may consume in the
future. It is not an implementation specification for an active integration.

Mother API does **not** currently depend on DIS for production behavior. It
currently provides no Aave or other protocol-intelligence capability through
DIS.

Keeping this boundary active records an accepted service-ownership decision. It
is not a product commitment, an implementation roadmap, or a promise that a
DIS-backed route will be delivered.

## Current implementation status

- No Aave-specific request types, response types, routes, clients, or tests
  are implemented in Mother API.
- No public or internal production route calls DIS.
- `src/adapters/dis/` contains dormant or exploratory infrastructure. Its
  retained client and tests concern a removed Polymarket prediction integration;
  they do not implement Aave or an active Mother API feature.
- Optional `DIS_*` configuration and the `/v1/status` configuration check do
  not make DIS a production dependency. The status check reports only local
  configuration/client construction; it is not a DIS network probe or evidence
  that a Mother API capability uses DIS.

## Architectural boundary

If activated by a future accepted scope, DIS owns protocol-specific data
resolution and protocol-domain logic. Possible read-only integrations could
include Aave, Compound, Meta Pool, Chainlink, or similar systems.

Mother API remains the public HTTPS and product-policy boundary. It may consume
an intentionally defined DIS capability, but it must not reimplement
protocol-specific calculations, archive access, reserve or market lookup, or
other DIS-owned protocol logic merely to expose a Mother API feature.

## Constraints for future activation

Any future DIS integration requires its own accepted scope and must define the
exact internal contract, tests, failure handling, and operational expectations
before it is implemented. Until then, this specification defines no DIS URL,
method, request type, response type, configuration guarantee, retry policy, or
test contract.

Future work must preserve these constraints:

- Protocol integrations remain read-only unless a future accepted specification
  explicitly changes that boundary.
- Mother API must not expose protocol capabilities publicly unless an accepted
  specification intentionally adds them and `CONTRACTS.md` is updated in the
  same change.
- An external-service failure, unavailable provider, or unverified response
  must never be represented as verified on-chain evidence.
- The consuming scope must state ownership and keep Mother API from taking on
  protocol indexing, protocol math, or other DIS responsibilities.

## Non-goals

This specification does not implement or promise Aave portfolio data, realized
yield, protocol positions, public portfolio routes, or a general
protocol-intelligence API. It also does not define a production DIS dependency,
an internal DIS contract, or a public wrapper for any DIS response.

`SPEC-005` remains a draft, future Aave portfolio proposal. Its dependency on
this boundary does not establish that either its portfolio endpoint or an Aave
DIS client exists.
