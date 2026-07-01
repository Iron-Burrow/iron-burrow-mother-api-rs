---
status: draft
owner: iron-burrow
last_reviewed: 2026-06-25
agent_edit_policy: update_when_relevant
---

# SPEC-011 — Public API Beta "v0.2" Surface v1

- The legacy FIFA endpoints have been removed from the contract and codebase
  for `v0.2.0`.
- Keep README.md aligned with the current public beta surface.

Goal: define the minimum stable public API surface needed to share Iron Burrow Mother API with early private beta users.

This spec should not mention Wavy Node directly. Treat the customer as “private beta client” or “early API consumer.” Wavy-specific readiness and relationship tracking belongs in Lu, not in the public Mother API repo.

## Context

Mother API is becoming the public-facing gateway for Iron Burrow on-chain intelligence. The first public beta should be intentionally small, honest, and stable.

We want to expose only two public endpoints at first:

1. Balances endpoint
2. ERC-20 transfer search endpoint

The goal is not to overbuild a full customer platform. The goal is to avoid anonymous, undocumented, unstable access and to provide a professional beta surface with clear contracts, API-key access, documented limits, OpenAPI docs, and explicit error behavior.

## Required Scope

Define the beta contract for:

* `POST /v1/balances/bulk`
* the final ERC-20 transfer search endpoint, likely `POST /v1/transfers/search` unless the current repo suggests a better route

For each endpoint, document:

* request DTO
* response DTO
* error DTO
* supported networks
* supported assets / contract filters
* limits
* authentication requirement
* known limitations
* example request
* example response
* example errors

## API Key Requirement

The public beta should require API-key access.

Minimum acceptable implementation:

* API keys can be generated manually/on demand.
* Each beta client receives a distinct key.
* Requests can be associated with the client/API key.
* Keys can be revoked.
* The system does not need a full self-serve customer portal yet.

Please inspect the current Mother API auth model and propose the smallest implementation that fits the existing architecture.

## Error Behavior

The API should fail explicitly instead of silently ignoring invalid values.

Required explicit errors include:

* unsupported network
* invalid network slug
* unknown asset slug
* native asset slug used where ERC-20 contract filtering is required
* invalid contract address
* too many subjects / addresses
* too many assets
* too many contract addresses
* ERC-20 transfer range too large
* upstream Bigwig/provider unavailable
* upstream Bigwig/provider timeout
* upstream Bigwig/provider error

Unknown slugs or invalid filters must not silently return empty results.

## Limits

Define public beta limits for both endpoints.

At minimum:

* max balance subjects
* max balance assets
* max ERC-20 contract addresses
* max ERC-20 block range
* timeout behavior
* supported networks
* whether timestamp ranges are supported now or deferred
* whether unfiltered ERC-20 transfer search is allowed

Use existing Mother API and Bigwig limits where possible. If the repo already defines limits, document them as contract truth. If limits are missing, propose conservative defaults.

## Observability

Define minimum logging/visibility requirements:

* API key/client identity
* endpoint
* timestamp
* network
* request size summary
* success/error status
* latency if available
* upstream error class when applicable

Do not log secrets or full sensitive payloads unnecessarily.

## Documentation

The implementation should update:

* OpenAPI annotations/schemas
* public API docs
* examples for both endpoints
* error examples
* known limitations

The docs should be honest: Mother API is early, bounded, and production-minded. Do not oversell capabilities.

## Landing Page Boundary

The Mother API repo does not need to implement the landing page unless this repo owns it. But the spec should state that public beta users should be referred to a landing page and formal API docs, not raw curl commands in chat.

Curl examples are acceptable inside documentation.

## Acceptance Criteria

The public beta is ready when:

* both endpoint contracts are stable enough for beta use
* API key protection exists
* requests are visible by client/API key
* public limits are enforced and documented
* explicit error behavior exists for invalid slugs, native asset misuse, unsupported networks, invalid contract addresses, and range limit violations
* OpenAPI docs include request, response, and error examples
* smoke tests cover happy paths and failure paths
* known limitations are documented

Please produce:

1. the SPEC draft,
2. implementation PR breakdown,
3. open questions,
4. blockers,
5. smoke test checklist.
