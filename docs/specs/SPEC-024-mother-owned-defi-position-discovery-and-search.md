---
status: draft
owner: iron-burrow
last_reviewed: 2026-08-18
agent_edit_policy: update_when_relevant
---

# SPEC-024 - Mother-Owned DeFi Position Discovery and Search

## Status and decision

When accepted and implemented, Mother API will own DeFi position discovery,
protocol-specific resolution, normalization, and response composition. Bigwig
will remain the controlled blockchain-read boundary, and Price Indexer will
remain the pricing and FX boundary.

Accepted [SPEC-029](SPEC-029-defi-protocol-realized-yield-lab-study.md)
introduces the shared canonical protocol registry for a bounded browser Lab
study. It does not accept, enable, or otherwise advance the discovery/search
operations proposed here.

This document proposes two protected public operations:

```http
POST /v1/defi-positions/discover
POST /v1/defi-positions/search
```

They are not part of the binding public contract, OpenAPI, or runtime route
surface until this specification is accepted, all acceptance blockers are
resolved, and the implementation change updates `CONTRACTS.md`.

## 1. Purpose and scope

DeFi positions are protocol-aware economic claims and liabilities. They are
not ordinary wallet token balances. This capability lets callers discover and
retrieve current positions for watch-only EVM accounts without asserting
account ownership or custody.

The initial protocol families under consideration are Aave V3, Morpho, and
Curve on Ethereum mainnet, plus Morpho on Base Mainnet. They are not enabled
integrations yet. No protocol, contract family, deployment, market, pool,
vault, or gauge is supported until it is recorded as verified in the accepted
registry scope and backed by a compiled Mother adapter.

### Goals

- Discover positions across all enabled integrations on an account's network.
- Retrieve complete current positions through explicit integration or known
  position selectors, without unbounded protocol fan-out.
- Pin complete results to canonical block evidence.
- Preserve assets, liabilities, collateral, LP claims, vault shares, and
  supported rewards without double counting.
- Return useful native position information when valuation is unavailable.
- Keep all RPC targets, ABI behavior, and execution bounds under Mother
  control.

### Non-goals

- Historical block or timestamp queries.
- Universal protocol, deployment, pool, vault, market, or gauge discovery.
- Caller-supplied contract addresses, ABI definitions, calldata, RPC methods,
  oracle addresses, or adapter kinds.
- Wallet balances, multi-account portfolio accounting, P&L, tax, yield
  forecasting, risk scoring, transaction construction, signing, or custody.
- Price derivation, FX logic, network RPC transport, or protocol indexing.

## 2. Service ownership

| Component | Responsibility |
| --- | --- |
| Mother API | Public DTOs, authorization, quotas, account normalization, registry loading, compiled adapter selection, canonical-block orchestration, protocol calls and interpretation, normalized positions, totals, partial results, and observability. |
| Bigwig | Controlled network RPC transport, canonical block reads, approved `eth_call`/batch execution, and transport/provider failures. |
| Price Indexer | Asset prices, supported quote currencies, FX conversion, and price/FX evidence. |

Mother adapters must use only verified registry targets and compiled ABI and
decoding behavior. A database row must never create executable support for an
unknown protocol or arbitrary contract call.

## 3. Common request rules

Both operations require a JSON `as_of` object:

```json
{
  "as_of": { "kind": "latest" }
}
```

`latest` is the only supported kind. `timestamp`, `block_number`, unknown
kinds, missing `as_of`, and extra temporal fields are rejected using the
existing validation-error conventions. A top-level `kind` is not accepted.

Both operations accept exactly one account selector:

```json
{ "account": { "network_slug": "eth-mainnet", "address": "0x..." } }
```

or:

```json
{
  "accounts": [
    {
      "network_slug": "eth-mainnet",
      "address": "0x...",
      "client_ref": "treasury-main"
    }
  ]
}
```

`account` and `accounts` are mutually exclusive; one is required; `accounts`
must be non-empty. Mother reuses the canonical `network_slug`, EVM address,
`client_ref`, duplicate-account, and reserved-network-alias validation used by
the balance DTOs. After validation, both forms become one ordered internal
account collection.

The accepted implementation must assign separate hard safety maximums and
operational limits for discovery and search. Until then, this draft does not
invent a maximum account count; the current balance limit of 50 is reference
context, not a DeFi limit.

## 4. Discovery

```http
POST /v1/defi-positions/discover
```

Discovery identifies which enabled, network-attached protocol integrations
contain positions for the requested accounts. It is the intentionally more
expensive, bounded fan-out operation.

### Request

```json
{
  "as_of": { "kind": "latest" },
  "account": {
    "network_slug": "eth-mainnet",
    "address": "0x0000000000000000000000000000000000000000"
  }
}
```

Discovery accepts no `protocols`, `positions`, `quote_currency`, target
addresses, pool/market/vault identifiers, adapter names, or operation
discriminator.

### Behavior and response

For each account, Mother resolves canonical evidence, loads every enabled
integration attached to that account's network, invokes its compiled adapter
within configured bounds, and returns lightweight descriptors for discovered
positions. Discovery reports every integration checked, including integrations
with zero positions.

A descriptor contains a deterministic `position_id`, account and network
reference, protocol slug, position type, stable protocol-native coordinates,
verified target identity, applicable participation kinds, evidence, and the
adapter/schema version required to refresh it. It does not return full legs,
metrics, totals, or valuation.

A `200 OK` response with an empty descriptor collection means no positions
were found in the enabled integrations that completed or reported zero
positions. It never claims universal DeFi coverage.

## 5. Search

```http
POST /v1/defi-positions/search
```

Search retrieves complete current economic positions. It accepts exactly one
of the following selector families.

### Account selector

```json
{
  "as_of": { "kind": "latest" },
  "accounts": [
    {
      "network_slug": "eth-mainnet",
      "address": "0x0000000000000000000000000000000000000000",
      "client_ref": "treasury-main"
    }
  ],
  "protocols": ["aave-v3"],
  "quote_currency": "USD"
}
```

The account selector requires exactly one of `account` or `accounts`, a
non-empty `protocols` list, and `quote_currency`. Mother invokes only the
selected protocols attached to each account's network; it must not expand the
request to every enabled integration.

### Known-position selector

```json
{
  "as_of": { "kind": "latest" },
  "positions": [{ "position_id": "dp1_..." }],
  "quote_currency": "USD"
}
```

The known-position selector requires a non-empty `positions` list and
`quote_currency`; it forbids `account`, `accounts`, and `protocols`. Mother
decodes each position reference and invokes only the identified registered
adapter and target. This is not global rediscovery.

The two selector families are mutually exclusive, and neither may be omitted.
Unknown protocol slugs, protocol/account network mismatches, malformed
references, disabled integrations, unsupported networks, stale references,
and removed targets receive explicit typed outcomes.

### Response model

The public envelope follows the repository's bulk-result pattern: it preserves
the input account reference and groups positions and integration outcomes by
account. Known-position requests return each encoded account reference in the
same grouped form. Complete positions include:

- deterministic `position_id`, account, `network_slug`, and protocol slug;
- position category and verified deployment, market, pool, vault, or gauge
  identity;
- normalized native economic legs and canonical asset identity where known;
- collateral flags, liabilities, and adapter-specific typed data;
- valuation status, quoted summary, evidence, and warnings.

Mother reuses established asset, amount, quoted-value, warning, error, and
evidence representations. Native integer and decimal amounts remain strings;
financial quantities never use floating-point arithmetic.

## 6. Evidence, valuation, and totals

Mother resolves one canonical block per unique network in a request and uses
that block for every complete adapter result for accounts on that network. The
evidence contains block number, block hash, and block timestamp. Adapters that
cannot complete at that block must report an incomplete outcome and cannot be
presented as deterministic.

If canonical evidence cannot be established for one network, the affected
accounts are failed explicitly while accounts on other networks may still
return valid results. If no requested account can establish evidence, the
request returns the established request-wide evidence-unavailable failure.

Search requires `quote_currency` and accepts only the existing `BTC`, `USD`,
`USDC`, and `MXN` values. Price Indexer supplies prices, FX, and associated
evidence. Discovery is unvalued and therefore does not accept a quote
currency.

Missing price or FX information produces an explicit partial valuation while
retaining the valid native position. Totals cover only positions returned by
this operation and must never be labelled complete if a material included
position cannot be valued. Mother must not combine these totals with ordinary
wallet balances.

Adapters and aggregation must prevent receipt-token/supplied-asset,
vault-share/underlying, LP-token/underlying, and wallet-LP/gauge-LP double
counting.

## 7. Registry and adapter model

Mother owns a PostgreSQL registry under `mother_api`. The accepted migration
will introduce a `defi_protocol` record for each concrete network integration
and a related `defi_protocol_target` record for its verified deployments,
markets, vaults, pools, or gauges.

`defi_protocol` must include a globally unique normalized public slug, network
foreign key, family, compiled adapter kind, adapter/schema version, enabled
state, verification state, bounded configuration, and timestamps.
`defi_protocol_target` must include a stable target key, target kind, canonical
validated address where applicable, verification state, target configuration,
and lifecycle state. A hybrid of relational identity fields plus versioned
adapter-specific JSONB configuration is permitted only when JSONB is validated
at write and startup, bounded, canonically serialized where identifiers need
it, and unable to select arbitrary calls.

Protocol slugs are global and network-bound. The intended, but not enabled,
Ethereum-mainnet slugs are `aave-v3`, `morpho`, and `curve`; the intended Base
Mainnet Morpho slug is `morpho-base`. An equivalent integration on another
network requires a distinct slug such as `aave-v3-base`; one slug can never
resolve across networks.

Compiled adapters define ABI signatures, calls, decoding, protocol
interpretation, normalized mapping, and hard execution bounds. Operational
registry changes may only register verified compatible targets, enable or
disable an integration, rotate verified metadata, or lower configured bounds.
They may not introduce adapter kinds or arbitrary executable behavior. Invalid
enabled configuration disables that integration at startup and emits an
operator-visible configuration failure; it must not make the entire service
accept unsafe configuration.

The initial adapter families are `aave_v3`, `morpho_blue_market`,
`morpho_vault`, `curve_pool`, and deferred `curve_gauge`. The public slug and
adapter kind are distinct. Aave groups supplied and borrowed legs by account
and deployment; Morpho distinguishes direct markets from vaults; Curve is
limited to explicit verified pool families. Gauge support, nested/metapool
claims, and every permissionless Morpho target remain deferred.

## 8. Position references

`position_id` is an opaque, deterministic, versioned `dp1_` reference. Its
base64url payload is a canonical serialized record containing the ID schema
version, normalized account address, network slug, protocol slug, adapter
schema version, stable target key, position type, and protocol-native position
key. Mother decodes the reference locally and resolves it through the current
registry; it does not persist every user's discovered positions.

The reference is not an authorization credential. A known-position request
for another watch-only account returns that reference's account group, subject
to the caller's network-scoped `defi_positions.search` capability. It cannot
alter the encoded account, network, adapter, or target, and it never permits
arbitrary RPC targets.

Malformed or unsupported-version references receive `invalid_position_id`.
References whose adapter schema has changed receive `stale_position_id` and
require rediscovery. Disabled integrations, unsupported networks, and removed
targets return their respective explicit per-position unavailable outcomes.
When a verified target remains supported but the current pinned-block read
finds no active position, search returns `position_not_found`; it does not
silently rediscover another target.

## 9. Authorization, limits, and failures

The accepted implementation adds separate capabilities
`defi_positions.discover` and `defi_positions.search`, enforced through the
existing owner, Client, API-key, and network-scope intersection. They have
separate quota accounting and request-limit policies because discovery is the
costlier fan-out operation.

The acceptance gate must assign hard and operational limits for accounts,
protocol selectors, known references, enabled integrations, targets inspected,
RPC and multicall items, per-adapter timeout, total deadline, concurrency,
returned descriptors, positions, legs, unique priced assets, payload size, and
discovery truncation. No numeric DeFi values are specified by this draft.

Request-wide failures cover malformed input, selector XOR violations,
unsupported temporal kinds, authorization, route disablement, and total
evidence failure. After a valid request, per-account and per-integration
outcomes use `complete`, `complete_with_zero_positions`, `partial`, `failed`,
`disabled`, `unsupported_on_network`, or `truncated`. A valid request for
which every selected adapter fails returns `200 OK` with top-level
`status: "failed"`, complete outcome diagnostics, and no positions; it is not
misrepresented as an empty successful search.

## 10. Security and observability

Mother validates canonical addresses and registry configuration before any RPC
work; bounds calldata, returndata, decoded arrays, positions, legs, and
response size; rejects malformed ABI results; validates token metadata and
decimals; propagates deadlines; and isolates individual target failures.
Logs and metrics must not expose API keys, authorization headers, provider
secrets, raw internal responses, or unnecessary account metadata.

Observability includes requested and executed integration counts, adapter and
Bigwig durations/outcomes, canonical-evidence failures, RPC/multicall counts,
discovered positions, returned legs, partial valuations, truncations, and
quota outcomes.

## 11. Test requirements

The implementation must include deterministic offline fixtures for every
enabled adapter and target. Tests must not depend on mutable chain state or
public RPC availability.

- Request validation: account/accounts XOR, empty and duplicate accounts,
  latest-only time, discovery selector rejection, search selector XOR, empty
  lists, unknown/disabled/network-mismatched protocols, and quote currency.
- Discovery: no enabled integrations, zero positions, multiple integrations,
  adapter failure isolation, truncation, checked-integration reporting,
  deterministic descriptors, and no caller-controlled targets.
- Search: account and known-reference selectors, no global fan-out, stale,
  malformed, disabled, removed, foreign-network, and no-longer-active
  references, deterministic ordering, and all-failed behavior.
- Evidence and valuation: one block per network, complete-result block
  consistency, evidence failure, missing price/FX, partial totals, and every
  stated no-double-counting invariant.
- Registry and configuration: globally unique slugs, network attachment,
  address/configuration validation, unknown adapter rejection, invalid enabled
  configuration, and enable/disable behavior.

## 12. Delivery phases

1. Resolve the acceptance blockers, registry schema, Bigwig primitive,
   capabilities, limits, position-ID encoding, and public error vocabulary.
2. Add the shared registry, adapter framework, canonical evidence orchestration,
   discovery/search DTOs, authorization, and deterministic ordering.
3. Add one verified Aave V3 Ethereum-mainnet adapter through both operations.
4. Add bounded Morpho direct-market then verified vault support for separate
   `morpho` Ethereum-mainnet and `morpho-base` Base-mainnet registrations,
   followed by explicit Curve pool families. Consider gauges only through a
   later accepted scope.
5. Complete configuration tooling, observability, performance tests, fixtures,
   public-contract updates, OpenAPI, and release documentation.

## 13. Public documentation on acceptance

The implementation that accepts this draft must add both operations to the
protected public route surface, generated OpenAPI, and `CONTRACTS.md`. It must
document the account/account-list XOR, search-selector XOR, latest-only
semantics, globally unique network-bound protocol slugs, separate discovery
fan-out and focused search behavior, partial and empty-result semantics,
evidence, limits, quote-currency behavior, enabled integrations, and the
absence of wallet balances, signing, and universal protocol coverage.

Until that coordinated implementation change, neither path is registered or
advertised as a public compatibility promise.

## 14. Acceptance blockers

This draft cannot be accepted or described as implementation-ready until all
of the following are recorded and verified:

1. Exact initial enabled integrations, canonical network records, contract
   families, target addresses, and verification sources.
2. The Bigwig canonical-block and bounded EVM-call primitives needed by the
   adapters, including failure and deadline semantics.
3. Registry DDL, target/configuration schema, validation location, and
   operator audit path.
4. Adapter versions, position-ID canonical encoding, error and warning codes,
   and public bulk response envelope.
5. Hard safety maxima and operational limits for discovery and search.
6. Capability grants, quota costs, rate-limit policy, and beta/alpha route
   registration policy.
7. Aave metric sources and units; supported Morpho market/vault families;
   supported Curve pool families; and any gauge scope.
8. Price Indexer asset-resolution and evidence mapping for every supported
   native position leg.
9. Fixture provenance and deterministic offline regression coverage.
10. Acceptance of this specification followed by coordinated `CONTRACTS.md`,
    OpenAPI, route, migration, documentation, and runtime implementation work.

Until then, no `/v1/defi-positions/*` route, public compatibility promise, or
enabled protocol integration exists.
