---
status: accepted
owner: iron-burrow
last_reviewed: 2026-08-19
agent_edit_policy: update_when_relevant
---

# SPEC-036 - Current Workspace Portfolio

**Product:** [Portfolio Workspace](../product/portfolio-workspace.md)
**Scope:** Private human web / application capability
**Persistence:** None

## 1. Summary

Mother shall provide an ephemeral, Workspace-scoped **Current Workspace Portfolio** observation.

The initial capability composes the latest supported wallet balances of the Workspace's registered member addresses into a single portfolio observation while preserving the provenance of those balances. It covers the active canonical asset mappings for each member's supported network and values available contributions in fixed USD, as defined in [Section 8.1](#81-initial-asset-coverage) and [Section 11](#11-valuation-and-quote-currency).

Conceptually:

```text
Workspace
  |
  +-- member address A (eth-mainnet)
  +-- member address B (base-mainnet)
  +-- member address C (eth-mainnet)
            |
            v
      BalanceSnapshotService
            |
            v
     Current Workspace Portfolio
            |
     +------+-------+
     |              |
   assets        valuation
```

Reading the Current Workspace Portfolio MUST NOT persist historical portfolio state.

Treasury Snapshot persistence remains a separate explicit capability.

---

## 2. Repository Alignment

This specification builds on, and does not alter:

* [RFC-003](../rfcs/RFC-003%20-%20Mother%20API%20El%20Vasco%20Architecture.md) and [ADR-001](../adr/ADR-001-human-and-machine-domain-strategy.md) for the shared Mother application boundary and private human-web delivery surface;
* [SPEC-020](SPEC-020-workspace-scoped-balance-and-transfer-views.md) for account-owned, selected-member balance views, which this draft composes but does not replace;
* [SPEC-023](SPEC-023-workspace-treasury-snapshots.md) for the separate, explicit immutable-snapshot boundary; and
* [SPEC-033](SPEC-033-in-memory-canonical-asset-registry.md) for canonical asset and `network_slug` identity.

It introduces no DeFi-position scope from draft SPEC-024 and no asynchronous-report scope from SPEC-035.

---

## 3. Motivation

Mother already knows how to:

* authenticate an `IBAccount`;
* authorize access to a Workspace;
* register watch-only Workspace member addresses;
* resolve supported balances;
* resolve current asset valuations.

What is missing is an application-level composition that answers:

> What does this Workspace collectively hold right now?

Users should not need to inspect each member address independently and mentally aggregate its balances.

The Current Workspace Portfolio provides that composition without introducing a new persisted portfolio entity.

---

## 4. Goals

This specification defines a capability that:

1. accepts an authorized Workspace;
2. discovers its registered Workspace member addresses;
3. obtains current balance observations for every active canonical asset mapped to each member network using existing balance capabilities;
4. aggregates equivalent canonical assets across member addresses where valid;
5. resolves current valuation using existing price infrastructure;
6. preserves source and network provenance;
7. represents incomplete observations truthfully;
8. exposes the resulting observation to the private Mother web application;
9. performs no portfolio-history persistence.

---

## 5. Non-Goals

This specification does not define:

* Treasury Snapshot persistence;
* automatic or scheduled snapshots;
* Aave account positions;
* NEAR validator positions;
* generic protocol-position discovery;
* historical portfolio reconstruction;
* arbitrary portfolio-source records;
* transaction signing or wallet custody;
* Workspace collaboration;
* `/v1` public API changes;
* caller-selectable or configurable quote currencies;
* portfolio performance, P&L, yield, or cost basis.

Those capabilities require separate product/specification decisions.

---

## 6. Terminology

### Workspace member address

The existing repository concept representing a registered watch-only EVM address belonging to a Workspace.

It remains the source configuration primitive for this specification.

This SPEC does not introduce a persisted `portfolio_source` abstraction.

### Current Workspace Portfolio

An ephemeral application-level observation representing the latest observable financial state of supported Workspace member addresses.

It is not a database entity.

### Member observation

The portion of the Current Workspace Portfolio attributable to one Workspace member address.

### Asset observation

An aggregated observation for one canonical asset across the Workspace.

### Valuation

The monetary value Mother can resolve for an observed asset amount using existing price infrastructure.

A valuation is an observation, not an assertion that every component of the Workspace has been successfully valued.

---

## 7. Authorization

The Current Workspace Portfolio is private Workspace data.

A caller MUST be authenticated as an `IBAccount`.

The caller MUST be authorized to access the requested Workspace according to the existing Workspace ownership rules.

Authorization MUST occur before portfolio composition.

Failure to authorize MUST NOT leak:

* member addresses;
* balances;
* asset composition;
* valuation;
* Workspace existence beyond existing authorization semantics.

No new ownership model is introduced by this specification.

---

## 8. Portfolio Resolution

For an authorized Workspace, Mother shall:

1. load the Workspace's registered member addresses;
2. resolve the latest supported balances for those members and the accepted asset coverage using the existing balance application capability;
3. retain the source member and network for each resolved observation;
4. map balances to canonical assets using existing canonical registry semantics;
5. aggregate balances representing the same canonical asset where aggregation is valid;
6. resolve current valuation using existing price infrastructure;
7. construct a `CurrentWorkspacePortfolio` application observation;
8. return that observation to the presentation layer.

Conceptually:

```text
Workspace
    |
    v
Workspace Members
    |
    +--------+--------+
    |        |        |
   ETH      Base     ETH
 member A  member B  member C
    |        |        |
    +--------+--------+
             |
             v
    BalanceSnapshotService
             |
             v
     Member Observations
             |
       +-----+-----+
       |           |
       v           v
   canonical     source
     assets      evidence
       |
       v
   aggregation
       |
       v
    valuation
       |
       v
Current Workspace Portfolio
```

The application layer owns this composition.

Askama templates MUST NOT independently resolve balances, aggregate assets, or calculate portfolio valuation.

### 8.1 Initial Asset Coverage

**Accepted decision:** Initial coverage is the canonical registry boundary.

The initial portfolio coverage is the canonical registry boundary. For each Workspace member, Mother MUST resolve every active canonical asset that has an active `asset_chain_map` for that member's `network_slug`. It MUST NOT scan a wallet for unknown ERC-20s or discover random, unsupported, or meme assets.

An active canonical asset without an active mapping on a member's network is not in that member's expected resolution set. Its absence MUST NOT make the portfolio partial.

The existing balance application capability remains the only balance-resolution mechanism. It accepts explicit asset-slug selectors, at most 20 selectors and 50 accounts per command, and at most 1,000 account-token resolutions. Planning MUST therefore be deterministic:

1. sort member network groups lexically by `network_slug`;
2. within each group, sort members by `(network_slug, lowercase address, member public_id)`;
3. select the active mapped asset slugs for that network and sort them by canonical `sort_order`, lowercase symbol, then `asset_slug`;
4. split members into chunks of at most 50 and asset selectors into chunks of at most 20; and
5. emit one existing `GetBalancesCommand` for each network, asset-selector chunk, and member chunk, in that order.

Every emitted command has at most 50 accounts, 20 token selectors, and 1,000 account-token resolutions. A Workspace may have up to 100 members, so multiple independently resolved commands are expected.

Each command is an independent latest observation. Mother MUST compose their results without manufacturing a common balance timestamp, block, provider observation point, or quote observation point. `CurrentWorkspacePortfolio.resolved_at` records only when Mother completed the composition.

The current catalog satisfies this policy through the existing asset-slug selector path: `ethereum` has active native mappings on both `eth-mainnet` and `base-mainnet`, which the balance resolver turns into its native target; every other active mapping on those two supported Workspace networks is ERC-20. No portfolio-specific native-asset or token-discovery abstraction is required.

This policy introduces no persisted `portfolio_source` or asset-selection record.

### 8.2 Existing Capability and Authorization Boundary

The initial portfolio reuses the existing `balances.read` capability. Before any balance-provider or price-provider call, Mother MUST confirm that the authenticated account has `balances.read` for every member network that the selected coverage would resolve. Missing authorization MUST fail the portfolio read rather than silently omitting that member and presenting an apparently partial portfolio.

The initial supported member networks remain `eth-mainnet` and `base-mainnet`, as enforced by the existing Workspace-member capability. This SPEC does not broaden that set.

---

## 9. Portfolio Observation

The exact Rust types are an implementation decision, but the application model should represent semantics equivalent to:

```text
CurrentWorkspacePortfolio
    workspace
    resolved_at
    quote_currency

    members[]
    assets[]

    known_value
    valuation_status
```

`resolved_at` is the time Mother completed portfolio composition. It MUST NOT claim that all member balances share a common block, timestamp, provider observation, or quote observation. Each contribution retains the existing per-account balance evidence, including its provider observation time and block evidence when available, and its existing USD quote outcome and quote timestamp when available.

### Members

Each member observation should preserve enough information to explain where portfolio assets originated.

Conceptually:

```text
MemberPortfolioObservation
    member_id
    network_slug
    address
    labels
    assets[]
    observation_status
```

### Assets

The aggregated asset view should represent equivalent canonical holdings across members.

Conceptually:

```text
AggregatedAssetObservation
    asset_slug
    total_amount
    known_value
    contributions[]
    valuation_status
```

A contribution identifies the underlying member/network observation from which the aggregate amount originated and retains its existing balance evidence, item outcome, and USD quote outcome.

The final implementation SHOULD reuse existing evidence and balance types rather than duplicating their semantics.

---

## 10. Asset Aggregation

Aggregation MUST operate on canonical asset identity, not ticker symbols or display names.

For example, two observations may only be combined when Mother can establish through its existing canonical registry semantics that they represent the same canonical asset.

Conceptually:

```text
member A / eth-mainnet
    5,000 USDC

member B / base-mainnet
    2,000 USDC

             ↓

Workspace USDC
    total_amount: 7,000
```

The aggregate MUST retain the underlying contributions:

```text
USDC
  total: 7,000

  contributions:
    eth-mainnet / member A: 5,000
    base-mainnet / member B: 2,000
```

Aggregation MUST NOT destroy provenance.

Assets whose canonical identity cannot safely be established MUST NOT be merged merely because their symbols match.

---

## 11. Valuation and Quote Currency

**Accepted decision:** USD is the sole initial quote currency and is fixed for every Current Workspace Portfolio.

The first implementation uses USD only:

```text
CurrentWorkspacePortfolio.quote_currency = USD
```

USD is fixed and is not caller-selectable or Workspace-configurable. This SPEC does not add a request parameter, account preference, persisted preference, or another quote currency.

Each resolved contribution reuses its existing USD Price Indexer quote outcome. An aggregated asset's `known_value` is the sum of its contributions with available USD quote values. The Workspace's `known_value` is the sum of successfully valued aggregated assets. Mother MUST NOT issue a new portfolio-level quote lookup or apply one common price to independently observed aggregate amounts.

The resulting value MUST NOT imply completeness when some portfolio components could not be observed or valued.

---

## 12. Partial Results

Partial-result truthfulness is a core requirement.

A Workspace Portfolio may be incomplete because:

* one member address could not be observed;
* one network/provider failed;
* one asset could not be resolved;
* one asset has no usable current price;
* another existing balance capability returned a partial result.

An active canonical asset that lacks an active mapping on a member's network is not an expected component and does not make the portfolio partial.

A successful partial observation SHOULD remain useful rather than failing the entire Workspace Portfolio whenever existing Mother semantics permit this.

Conceptually:

```text
known_value: 12,432 USD
valuation_status: partial
```

This means:

> Mother successfully values the observed and valuatable portion of the portfolio at 12,432 USD.

It MUST NOT mean:

> The complete Workspace is worth exactly 12,432 USD.

The application model MUST preserve enough information for the presentation layer to explain material omissions.

---

## 13. Observation Status

The implementation should define a small explicit status model.

Conceptually:

```text
complete
partial
unavailable
```

`complete` means all expected supported member observations and valuations required by this SPEC succeeded.

`partial` means a useful portfolio observation exists but one or more expected components are unavailable or unvalued.

`unavailable` means Mother cannot produce a meaningful current portfolio observation.

The implementation SHOULD align this status model with existing Mother partial-result semantics where possible rather than introducing parallel error vocabulary.

---

## 14. Empty Workspace

A Workspace with no registered member addresses is valid.

Its Current Workspace Portfolio should be represented as an empty portfolio rather than an infrastructure failure.

Conceptually:

```text
members: []
assets: []
known_value: 0
```

The presentation layer may use this state to invite the user to register their first Workspace member address.

---

## 15. Persistence Boundary

Resolving a Current Workspace Portfolio MUST NOT:

* insert a Treasury Snapshot;
* create portfolio-history rows;
* create report rows;
* create an implicit Workspace event representing historical capture;
* otherwise persist the financial observation as historical state.

In particular, the portfolio route and application service MUST NOT call the Workspace activity `append_observation` path used by the selected member-address balance and transfer views.

Normal operational telemetry, request accounting, logs, caches, or existing infrastructure behavior are not considered portfolio-history persistence.

Historical capture requires an explicit Treasury Snapshot operation governed separately.

The conceptual boundary is:

```text
Current Workspace Portfolio
        |
        | read
        v
    ephemeral


Current Workspace Portfolio
        |
        | explicit future capture
        v
Workspace Treasury Snapshot
        |
        v
    immutable
```

---

## 16. Presentation

The private Mother web application should expose the read-only Workspace portfolio view at:

```text
GET /workspaces/{workspace_id}/portfolio
```

It follows the existing Workspace browser-session, account-ownership, private/no-store HTML, and cross-account `404` conventions. It has no JSON export, no `/v1` route, and no state-changing form.

The initial view should make visible:

* Workspace identity;
* current known portfolio value;
* observation/valuation status;
* aggregated assets;
* member/source contributions;
* meaningful partial-result warnings.

The UI should favor portfolio comprehension over infrastructure details while retaining sufficient evidence for the user to understand incomplete observations.

Askama remains a presentation layer over an application-level portfolio model.

---

## 17. Initial Supported Sources

This specification composes only existing supported Workspace member addresses.

Initial networks are therefore constrained by the existing Workspace-member and balance capabilities.

The existing implementation admits:

* `eth-mainnet`;
* `base-mainnet`.

This SPEC does not expand supported Workspace-member networks.

---

## 18. Future Composition

The Current Workspace Portfolio is intentionally designed so that future accepted capabilities may contribute additional observations.

Potential future contributors include:

```text
Current Workspace Portfolio
    |
    +-- Wallet balances
    |
    +-- DeFi positions
    |     +-- Aave
    |     +-- Morpho
    |     +-- Curve
    |
    +-- Infrastructure positions
          +-- NEAR validator
```

These are not implemented or authorized by this SPEC.

Future composition must address issues such as double counting before protocol positions can safely participate in total portfolio valuation.

For example, wallet collateral deposited into Aave must not simultaneously appear as both freely held wallet assets and protocol exposure if doing so would misrepresent economic ownership.

---

## 19. Invariants

The implementation MUST preserve the following invariants:

1. A portfolio observation belongs to exactly one authorized Workspace.
2. Workspace member configuration remains the source of wallet membership.
3. Reading a portfolio does not persist portfolio history.
4. Asset aggregation uses canonical identity rather than display symbols.
5. Aggregation does not destroy member/network provenance.
6. Partial observations are represented truthfully.
7. Known valuation is not presented as complete valuation when unresolved components exist.
8. Askama does not own portfolio composition.
9. No new `/v1` contract is introduced.
10. No generic persisted `portfolio_source` entity is introduced.
11. No Aave or NEAR position semantics are introduced by this SPEC.
12. The portfolio quote currency is always USD.
13. Independently resolved contributions retain their own balance evidence and quote outcomes.

---

## 20. Testing Expectations

Tests should cover at minimum:

### Authorization

* owner can inspect the Workspace portfolio;
* another IBAccount cannot inspect it.

### Empty Workspace

* Workspace without members produces a valid empty observation.

### Single Member

* one member's supported balances produce the expected portfolio assets and valuation.

### Multiple Members

* balances across multiple members compose into one Workspace observation.

### Multiple Networks

* canonical assets across supported networks aggregate correctly when canonical identity permits it.

### Resolution Planning

* active canonical asset mappings produce deterministic, network-aware asset-selector groups ordered by canonical identity;
* native `ethereum` mappings and ERC-20 mappings use the existing asset-slug balance-selector path;
* unmapped canonical asset/network pairs are excluded from the expected set without degrading the portfolio;
* no group exceeds the 50-account, 20-selector, or 1,000-resolution-item limits;
* a 100-member Workspace remains resolvable according to the accepted partitioning policy; and
* independently resolved groups retain their own balance evidence and quote observations rather than manufacturing a common observation point.

### Provenance

* aggregated assets retain their underlying member/network contributions.

### Partial Balance Resolution

* one unavailable member does not incorrectly present the resulting portfolio as complete.

### Partial Valuation

* an unpriced asset remains visible;
* known valuation remains available where possible;
* known value is the sum of available contribution USD quote values rather than a re-quoted aggregate amount; and
* overall valuation status becomes partial.

### Persistence

* reading the Current Workspace Portfolio does not create a Treasury Snapshot or other historical portfolio record.
* reading the Current Workspace Portfolio does not append a Workspace activity/evidence event.

### Presentation

* the portfolio route requires an authenticated browser session;
* an owner can inspect the portfolio and another account receives the existing not-found behavior;
* missing `balances.read` for any required member network prevents provider calls and does not silently omit the member;
* rendered values preserve partial-result warnings and contribution provenance; and
* the route performs no state-changing database operation.

---

## 21. Acceptance Criteria

This specification is ready for the implementation sequence in [Section 22](#22-implementation-plan) with its initial asset-coverage and quote-currency decisions recorded. It is complete when an authenticated user can open an authorized Workspace and receive a current wallet-only USD portfolio observation derived from its registered member addresses.

The observation must:

* compose existing balance capabilities;
* cover every active canonical asset mapped to each member network without arbitrary token discovery;
* aggregate canonical assets safely;
* preserve source provenance;
* provide current known USD valuation from contribution quote outcomes;
* communicate partial results truthfully;
* remain ephemeral;
* require no `/v1` contract change;
* introduce no Aave, NEAR, scheduling, or Treasury Snapshot persistence behavior.

At that point, Mother can answer its first Workspace-level portfolio question:

> **What does this Workspace collectively hold right now?**

---

## 22. Implementation Plan

The following PRs are deliberately ordered so that portfolio composition is proven before a browser route exposes it. Each PR must preserve the existing `/v1` and OpenAPI surface.

### PR 1 - Resolve the specification review decisions

* Record the accepted Sections 8.1 and 11 decisions in this SPEC and the Portfolio Workspace product document.
* Keep this documentation-only PR free of planner code and tests, a route, database migration, portfolio persistence, and `CONTRACTS.md` change.

### PR 2 - Add deterministic resolution planning

* Implement the deterministic asset-selection and limit-partitioning rules that turn Workspace members into valid existing balance-service commands.
* Add focused planner tests for active catalog mappings, native and ERC-20 asset-slug selectors, unmapped asset/network pairs, limit boundaries, stable ordering, and a 100-member Workspace.
* Preserve independent command evidence and quote outcomes; do not add a route, database migration, or persistence behavior.

### PR 3 - Add the application-level portfolio observation and resolver

* Introduce an application-level `CurrentWorkspacePortfolio` model and a resolver that consumes owned Workspace members, the canonical registry, and the existing balance service.
* Reuse existing balance outcomes, evidence, decimal handling, and Price Indexer quote outcomes; aggregate only by canonical asset identity and retain every member/network contribution.
* Implement explicit `complete`, `partial`, and `unavailable` composition rules, including valid empty Workspaces and USD known-value semantics derived from available contribution quote values.
* Add unit and service tests for aggregation, provenance, balance-provider degradation, unavailable prices, and the no-common-observation-point rule for balances and quotes.
* Do not add a migration, Treasury Snapshot write, Workspace activity event, or HTTP route in this PR.

### PR 4 - Enforce Workspace and capability authorization around resolution

* Wire the resolver through the existing account-owned Workspace lookup and `balances.read` checks for every required member network before any upstream call.
* Preserve the established redirect, `404`, and service-unavailable behavior for Workspace browser flows without exposing member, balance, or valuation data across accounts.
* Add route-adjacent/service tests proving authorization precedes provider access and that no unauthorized member is silently dropped from a result.
* Add an explicit regression test that a portfolio read performs no Treasury Snapshot insert and no Workspace activity append.

### PR 5 - Deliver the private Workspace portfolio page

* Add `GET /workspaces/{workspace_id}/portfolio`, link it from the Workspace page, and render it with an Askama template that consumes only the application-level portfolio model.
* Render empty, complete, partial, and unavailable states with known-value qualification, aggregate assets, source contributions, and material omission warnings.
* Add browser-route and template tests for session requirements, ownership isolation, private/no-store response headers, empty state, partial warnings, and absence of mutations.
* Update `CONTRACTS.md` in this same PR to list the new private HTML route; do not add a `/v1` operation or a JSON export.

### PR 6 - Review hardening and release verification

* Re-read the implementation against this SPEC, `docs/product/portfolio-workspace.md`, SPEC-020, SPEC-023, and the accepted registry boundaries; remove any accidental snapshot, activity, DeFi-position, or public-API coupling.
* Add the completed implementation note to `HISTORY.md` and keep the product document aligned with the implemented first coverage policy.
* Run:

```sh
cargo fmt
cargo test
make test-db-postgres
make smoke-db-migrate
git diff --check
```
