---
status: accepted
owner: iron-burrow
last_reviewed: 2026-08-06
agent_edit_policy: update_when_relevant
---

# SPEC-029 - DeFi Protocol Realized Yield Lab Study

## Purpose

Provide an authenticated, browser-native `/lab/defi-protocols/realized-yield`
study for deterministic historical realized yield. It is an evolving human
product transport, not a `/v1` operation or an OpenAPI contract.

## Scope

The study accepts only a canonical `protocol_slug`, an asset slug, two block
numbers, and the optional APY choice. It never accepts network identity,
chain ID, contract addresses, RPC endpoints, ABI definitions, or calldata.
Mother resolves all blockchain configuration from its verified canonical
protocol registry and invokes only a compiled adapter through Bigwig.

The initial supported protocol is `aave-v3` on `eth-mainnet`, with USDC,
USDT, DAI, and GHO. Its adapter computes reserve normalized-income yield and
an optional timestamp-based APY without floating-point arithmetic. Timestamp
failure is a warning, not a failure of realized-yield computation.

## Registry and extension

Mother owns `mother_api.defi_protocol` and `mother_api.defi_protocol_target`.
Each enabled protocol is network-bound, verified, and assigned a compiled
adapter kind and version. Targets are verified bounded configuration; registry
data cannot create arbitrary calls or executable adapter behavior.

Future protocol slugs require a verified registry entry and compiled adapter,
but reuse this study route. This specification does not enable position
discovery, search, valuation, public API exposure, or any protocol other than
the initial Aave V3 slice.
