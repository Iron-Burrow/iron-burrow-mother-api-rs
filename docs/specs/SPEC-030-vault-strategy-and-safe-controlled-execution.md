---
status: draft
owner: iron-burrow
last_reviewed: 2026-08-07
agent_edit_policy: update_when_relevant
tags: python
---

# SPEC-030 - Vault Strategy and Safe-Controlled Execution 🐍

## Motivation and decision

Mother API needs a bounded, auditable way to observe a USDC yield vault,
compute allocation recommendations, validate them independently, and prepare
approved actions for authorized Safe execution.

This is a modular capability within the single Mother runtime and PostgreSQL
database:

```text
Observation → Strategy → Proposal → Validation → Safe execution
```

It creates no strategist service, executor service, database, public `/v1`
route, hot-wallet signer, or permanently running Python process.

Repository conflict: Mother is currently documented as not being an execution
service, and Morpho support exists only as unimplemented scope in draft
[SPEC-024](SPEC-024-mother-owned-defi-position-discovery-and-search.md). This
SPEC must not be accepted or implemented until SPEC-024's Mother-owned registry
and compiled-adapter direction is accepted. Its implementation must amend the
active README and agent scope to describe this narrow Safe-controlled exception.

## Initial scope

The first vertical slice supports exactly one configured `eth-mainnet` USDC
strategy vault:

- The vault is an existing Safe address, registered as a member of one owning
  Workspace.
- The owning IBAccount receives explicitly granted `vaults.read` and
  `vaults.approve` capabilities; no broad automatic grant is made.
- The vault holds and redeems USDC through its existing Safe operations. Mother
  does not custody deposits, operate a pooled user vault, or implement
  deposit/withdrawal smart contracts.
- Allocation is limited to verified, allowlisted Morpho Blue USDC markets. No
  borrowing, collateral, leverage, swaps, cross-chain transfers, non-USDC
  assets, or directional hedges are permitted.
- Evaluation runs weekly in the Mother process. It produces a proposal for
  browser review; an approved proposal is independently revalidated by Rust and
  prepared as a Safe transaction package for external owner signing and
  submission.

A future pooled user-deposit vault requires a separately accepted and audited
smart-contract scope.

## Architecture and module boundaries

| Module | Responsibility |
| --- | --- |
| Observation | Use Mother's verified protocol registry, compiled Morpho adapter, and Bigwig read boundary to capture a canonical, normalized vault and market snapshot. |
| Vault strategist | Invoke a fixed Python tool on demand with versioned structured input; receive a versioned allocation recommendation. |
| Proposal service | Persist immutable input/output evidence, proposal lifecycle events, review decisions, and expiry. |
| Policy and execution planner | Re-observe state, enforce all Rust-side policy, derive bounded Morpho and ERC-20 calls, and create an immutable Safe execution plan. |
| Safe handoff | Render a typed Safe transaction package for external Safe owners. Mother does not sign, submit, or retain transaction private keys. |
| Browser/Lab | Provide private no-store pages at `/lab/vaults/{vault_id}` and proposal review actions under that vault. No `/v1` or API-key transport is added initially. |

Python is compute-only: it receives JSON on stdin and returns one JSON result
on stdout. Mother invokes only a deployment-configured executable for a
registered strategy version, with a fixed working directory, scrubbed
environment, timeout, output-size limit, and no database, Bigwig, signer, or
wallet credentials. Invalid, oversized, timed-out, or schema-incompatible
output fails the evaluation and cannot create a proposal.

## Evidence, contracts, and persistence

Each scheduled run captures one canonical Ethereum block and persists an
immutable normalized observation containing:

- vault Safe USDC balance and direct-Morpho market positions;
- verified market identity, liquidity, utilization, supply-rate/expected-APY
  inputs, and adapter/version evidence;
- block number, hash, timestamp, observation schema version, and canonical
  input digest.

The initial slice intentionally excludes historical-yield volatility,
realized-yield analytics, gas-price estimation, benchmarking, and backtesting
because Mother does not currently supply Morpho historical metrics or
transaction/gas data. The weekly immutable observations establish the evidence
needed to add those calculations later.

Introduce a small vault domain with immutable evidence records:

- `vault`: configured Workspace member/Safe, `network_slug`, USDC asset
  identity, active strategy and policy version.
- `vault_observation`: append-only normalized snapshot and evidence digest.
- `vault_strategy_proposal`: immutable strategy input/output, strategy/tool
  version, rationale, expected metrics, target allocation, expiry, and source
  observation.
- `vault_strategy_event`: append-only review, rejection, expiry, validation,
  preparation, submission-reference, and reconciliation events with actor
  identity.
- `vault_execution_plan` and `vault_execution_result`: immutable validated
  call package and observed execution outcome.

Use opaque public IDs, versioned JSON payloads, timestamps, and database
no-update/no-delete triggers for evidence and lifecycle events. Do not retain
raw Bigwig, Python stderr, Safe credentials, API keys, or private keys.

`VaultStrategyInputV1` contains the vault/policy/strategy versions, normalized
USDC amounts as strings, current allocation, allowlisted market observations,
canonical evidence, and input digest. `VaultStrategyProposalV1` contains a
reason code and explanation, current and target allocations, optional expected
APY metrics as decimal strings, assumptions, source-input digest, and expiry.
Rust rejects unknown fields, floating-point values, unknown markets, mismatched
digests, and any output that does not exactly total the permitted allocation.

## Validation, signing, and scheduling

Rust validates a proposal again against a fresh canonical observation before
creating an execution plan. The initial deterministic policy is limited to:

- the configured active Safe, `eth-mainnet`, USDC, verified Morpho protocol,
  and configured direct markets;
- configured minimum liquid USDC allocation, per-market caps, approved
  market/oracle/collateral configuration, and minimum expected improvement;
- proposal expiry and exact source-observation compatibility;
- current balances, available liquidity, allowance requirements, and bounded
  withdrawal/supply amounts;
- Rust-derived target addresses, calldata, token approvals, operation count,
  and value bounds.

Policy values are versioned vault configuration, never Python output. Exact
values are not invented in this draft; no vault may be enabled until they are
approved with its verified targets.

The first signing boundary is deliberately human-controlled:

```text
approved proposal
  → Rust validation and Safe transaction package
  → external Safe owner review/signing/submission
  → Mother observes confirmed resulting state
```

Mother does not call `eth_sendRawTransaction`, hold an EOA or Safe owner key,
or integrate an automated signer. A submitted transaction hash may be recorded
only after operator entry or trusted reconciliation; final execution status
requires final-chain evidence.

Mother starts one config-gated weekly evaluation task with the HTTP runtime. A
PostgreSQL lease and run record prevent concurrent evaluation of the same vault.
There is no generic event engine, queue service, or Python daemon. Failed runs
create an observable failure record and wait for the next scheduled run; they
do not retry or execute automatically. Future market/risk triggers remain
deferred.

## Failure behavior, observability, and tests

No proposal is created when canonical evidence, verified configuration,
required market data, Python execution, or persistence fails. Stale,
superseded, rejected, or validation-failed proposals can never be prepared.
Missing Safe confirmation leaves the proposal prepared/submission-pending;
uncertain on-chain status remains explicitly unreconciled rather than inferred.

Record structured logs and metrics for evaluation runs, observation freshness,
adapter and Python duration/outcome, proposal state transitions, validation
rejection reasons, Safe-package creation, and reconciliation status. Redact
addresses beyond necessary audit records, secrets, Python stderr, API keys, and
transaction-signing material.

Testing must include:

- deterministic offline Morpho adapter fixtures and canonical evidence
  consistency;
- Python input/output schema, timeout, malformed-output, unsupported-version,
  and hostile-output tests;
- Postgres append-only, lifecycle, ownership, capability, and lease-concurrency
  tests;
- policy tests for every prohibited target, stale state, cap, liquidity,
  allowance, and allocation-total violation;
- deterministic Safe-package construction and browser CSRF/authorization tests;
- end-to-end fixture coverage from observation through prepared execution plan,
  without live-chain mutation.

Run Rust/Postgres regression coverage with `make test-db-postgres`; plain
`cargo test` must remain safe without a disposable database.

## Delivery phases and acceptance blockers

1. Accept SPEC-024's registry/compiled-adapter model and extend it for verified
   Morpho Blue direct-market configuration; add the bounded read primitives
   needed for deterministic position and final transaction observation.
2. Add the vault domain, immutable evidence/lifecycle schema, Workspace-bound
   authorization, weekly lease worker, and one verified Safe configuration.
3. Add the Rust Morpho observation and execution-planning adapter, fixed Python
   tool contract, simple allocation strategist, and private Lab review flow.
4. Enable one vault only after deterministic fixtures, Safe handoff rehearsal,
   operational runbook, and all policy controls pass review.

Before enablement, record and verify:

- the Safe address, Safe version, external owner/signing procedure, and owning
  IBAccount/Workspace;
- exact USDC deployment, Morpho Blue market identifiers and addresses,
  oracle/collateral configuration, and verification sources;
- the vault policy's liquidity floor, market caps, improvement threshold,
  freshness window, transaction bounds, and approval/allowance policy;
- the authoritative Morpho supply-rate calculation and units;
- the Bigwig finality/receipt-read semantics used for reconciliation.

Future work may add multiple vaults or stablecoins, Morpho vault adapters,
historical realized-yield and volatility analysis, benchmarks/backtests, event
triggers, automatic constrained execution, BTC/ETH hedges, and other
protocols. Each requires separately accepted scope and must preserve:

> Mother observes. Python computes. Strategy proposes. Rust validates.
> Authorized execution acts.
