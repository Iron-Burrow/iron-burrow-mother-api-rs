---
status: draft
owner: iron-burrow
last_reviewed: 2026-08-19
agent_edit_policy: update_when_relevant
---

# Portfolio Workspace

## Purpose

Portfolio Workspace is the intended private Mother web-product journey:

```text
IBAccount → Workspace → registered sources → Current Workspace Portfolio → Treasury Snapshots
```

This document records product intent. It does not define API contracts,
persistence schemas, provider integrations, or scheduling behavior. Those
belong in accepted RFCs and focused SPECs.

## Current product building blocks

Mother already provides:

- authenticated `IBAccount` entry and browser sessions;
- account-owned Workspaces, including multiple active Workspaces;
- watch-only **Workspace member addresses** on Ethereum mainnet and Base
  mainnet;
- private member-address label sets;
- selected-address balance and transfer views;
- manual, immutable Workspace Treasury Snapshots of selected balance
  observations; and
- private Workspace activity and snapshot-history views.

A newly created IBAccount receives one initial `Personal Workspace`.
There is no Workspace-level Current Portfolio view, no portfolio-source
record, no Aave account-position capability, no NEAR validator capability,
and no automatic snapshot scheduling.

## Product concepts

### IBAccount and Workspace

An IBAccount owns one or more Workspaces. A Workspace is the private durable
boundary for user-controlled configuration and historical observations.

The onboarding experience creates one initial personal Workspace. This does
not imply a single-Workspace limit, Workspace sharing, ownership transfer, or
organization semantics.

### Registered sources

The current registered source is a **Workspace member address**: a watch-only,
network-scoped EVM address with optional client reference and private labels.
Registration never asserts address ownership, custody, or signing authority.

“Portfolio source” is a product grouping term, not a new persisted object.
Future source kinds require their own accepted scope before becoming Workspace
configuration.

### Current Workspace Portfolio

A Current Workspace Portfolio is a planned, ephemeral observation of the
registered sources in one Workspace. Reading it must not create historical
state.

The first possible composition is the latest supported wallet-balance
observation for Workspace member addresses. It must preserve source, network,
evidence, valuation availability, and partial-result truthfulness.

Aave account positions and NEAR validator positions are not part of the
current portfolio capability. They remain future possibilities only after
their respective discovery and read boundaries are accepted and implemented.

### Treasury Snapshot

A Treasury Snapshot is an explicitly persisted, immutable Workspace
observation. It answers what the selected Workspace observation looked like
when captured; it is not a side effect of viewing the Current Workspace
Portfolio.

Current snapshots are manual balance observations with the boundaries defined
by SPEC-023. A later product slice may make snapshots capture an agreed
Current Workspace Portfolio composition, while retaining the meaning and
readability of existing snapshots.

Treasury Snapshots are not generic reports. The separate asynchronous-report
mechanism is not a prerequisite for manual Workspace snapshots.

## Product boundary

Included today:

- private IBAccount and Workspace ownership;
- watch-only Ethereum-mainnet and Base-mainnet member addresses;
- private address labels;
- selected-address balance and transfer views; and
- manual immutable treasury snapshot history.

Planned, subject to focused specifications:

- a current Workspace-level wallet portfolio observation;
- explicit capture of that observation as a Treasury Snapshot; and
- later, composition of supported discovered protocol positions.

Not currently committed:

- Aave account-position presentation;
- NEAR validator positions;
- a daily automatic Treasury Snapshot;
- arbitrary schedules, report builders, notifications, exports, tax reporting,
  custody, signing, or Workspace collaboration.

Automatic snapshots require an accepted scheduling-ownership decision. Mother
does not introduce an in-process scheduler; refresh scheduling remains outside
Mother’s responsibility.

## Product principles

- Workspace configuration is distinct from resolved observations.
- Current observations are ephemeral; persisted history is intentional.
- Canonical assets, networks, and verified protocol facts are not user-owned
  Workspace data.
- Private Workspace data remains account-owned and is isolated by ownership
  checks.
- Human pages are Askama presentations over application services. They do not
  assemble portfolio semantics directly from infrastructure adapters.
- A web-product capability does not imply a `/v1` API capability.

## Related repository decisions

- RFC-003 and ADR-001 define Mother’s product, ownership, and human-web
  boundaries.
- RFC-006, SPEC-033, and SPEC-034 define immutable canonical and verified
  protocol registries.
- SPEC-020 and SPEC-021 define current Workspace views and evidence history.
- SPEC-023 defines current manual treasury snapshots.
- SPEC-024 is a draft proposal for Mother-owned DeFi position discovery; it
  does not yet provide Aave positions.
- SPEC-035 defines a separate, currently inactive asynchronous-report
  foundation.

## Open product decisions

- Should the first Current Workspace Portfolio expose a single quote currency,
  and how should the product present partial valuation?
- After a supported position capability exists, which position categories may
  participate in a Workspace portfolio without double counting wallet and
  protocol exposure?
- Which external component requests daily snapshot capture while preserving
  Read Model ownership of scheduling?
