---
status: archived
owner: iron-burrow
last_reviewed: 2026-08-06
agent_edit_policy: do_not_update
---

> Archived: 2026-08-06
>
> Status: Archived
>
> Reason: The proposed Mother API to DIS protocol-intelligence integration is
> no longer intended. Mother-owned DeFi position discovery and resolution is
> now proposed by [SPEC-024](../specs/SPEC-024-mother-owned-defi-position-discovery-and-search.md).
>
> Evidence:
> - Mother API has no production DIS-backed route, Aave integration, or DIS
>   protocol-intelligence dependency.
> - SPEC-024 records the replacement product direction; it remains a draft and
>   does not itself create a public API promise.
>
> Notes:
> - This document is retained only as historical context.
> - Its proposed DIS integration will not be implemented or supported.
> - Public API contracts remain unchanged until a later accepted and
>   implemented specification updates them.

# SPEC-001 - DIS Protocol Intelligence Boundary

## Historical purpose

This specification formerly preserved a possible architectural boundary in
which `iron-burrow-defi-intelligence-service` (DIS) would own protocol-specific
read-only intelligence that Mother API might consume.

That direction is archived. It defines no current or future Mother API
dependency, internal DIS contract, route, request or response type,
configuration guarantee, retry policy, fixture contract, or public wrapper.

## Historical implementation status

- Mother API never implemented an Aave-specific route, request type, response
  type, or active DIS-backed protocol-intelligence capability.
- `src/adapters/dis/` and optional `DIS_*` configuration are retained dormant
  infrastructure from earlier experiments. They do not establish a production
  dependency or support this archived design.
- The `/v1/status` DIS configuration check reports only local client
  construction; it is not a network probe or evidence of a production feature.

## Historical boundary

The former proposal assigned protocol discovery, contract reads, protocol math,
and protocol-domain logic to DIS. Mother would have consumed a versioned
internal capability and composed a public response.

The replacement direction is Mother-owned DeFi position discovery and search
in draft [SPEC-024](../specs/SPEC-024-mother-owned-defi-position-discovery-and-search.md). Under
that proposal, Bigwig remains the controlled blockchain-read boundary and
Price Indexer remains the price and FX boundary. The change does not make
SPEC-024 accepted or implementation-ready.

## Historical non-goals

This archived document never implemented or promised Aave portfolio data,
realized yield, protocol positions, public portfolio routes, or a general
protocol-intelligence API.

Archived [SPEC-005](SPEC-005-aave-v3-portfolio-estimate.md) preserves the
unimplemented ETH Mexico hackathon portfolio proposal. It must not be read as
evidence that this DIS direction was implemented.
