---
status: draft
owner: iron-burrow
last_reviewed: 2026-08-06
agent_edit_policy: update_when_relevant
---

# SPEC-028 - ib_account Default Quote Currency Fallback for Protected Calls

## Status and decision

This draft proposes account-scoped quote-currency fallback for protected
balance and DeFi-search calls. It introduces an `ib_account` default quote
currency setting so account-owned callers can omit `quote_currency` in request
bodies and still receive deterministic valuation behavior.

This spec is not binding public contract, OpenAPI truth, or runtime behavior
until accepted and implemented with coordinated updates to `CONTRACTS.md`,
OpenAPI, migrations, and regression tests.

## 1. Purpose and scope

### Purpose

Account-owned API keys and delegated client keys should be able to rely on a
configured account default quote currency when calling protected operations.
This reduces request payload friction while preserving strict validation,
determinism, and existing quote-currency allowlists.

### In scope

- Add a durable default quote currency setting to `mother_api.ib_account`.
- Define deterministic quote-currency resolution precedence for protected
  account and agent keys.
- Allow protected callers to omit `quote_currency` (or send blank/
  whitespace-only input) and resolve to account default.
- Preserve existing quote validation and normalization.
- Apply behavior to:
  - `POST /v1/balances`
  - `POST /v1/balances/bulk`
  - future protected `POST /v1/defi-positions/search` behavior under
    accepted SPEC-024 scope.

### Out of scope

- Adding new quote currencies beyond `USD`, `MXN`, `USDC`, `BTC`.
- Changing quote provider ownership boundaries (Price Indexer remains owner).
- Enabling fallback for anonymous, legacy consumer, or unauthenticated routes.
- Introducing a public self-service account settings endpoint in this slice.

## 2. Ownership and boundaries

| Component | Responsibility |
| --- | --- |
| Mother API | Stores account default setting, resolves effective request quote currency, preserves typed error behavior, and logs resolved request context. |
| Price Indexer | Continues owning quote availability, FX conversion, and quote evidence for the resolved quote currency. |
| Bigwig | Unchanged; no quote-resolution ownership. |

No service boundary changes are introduced.

## 3. Data model changes

Add a new column on `mother_api.ib_account`:

- `default_quote_currency text not null default 'USD'`
- constraint allowed values: `USD`, `MXN`, `USDC`, `BTC`

### Rationale

- `NOT NULL` + default gives deterministic fallback for all existing and future
  accounts.
- DB check constraint prevents unsupported runtime configuration.
- Existing account rows become immediately valid without manual backfill.

## 4. Effective quote currency resolution

For protected route handlers that currently require body `quote_currency`,
resolve an effective value using this precedence:

1. If request `quote_currency` is present and, after trimming, non-empty:
   - use request value
   - normalize to uppercase
   - validate against allowlist
2. Else, if authenticated principal has `ib_account_id` and key kind is
   account-owned (`account` or delegated `agent`):
   - use the account's `default_quote_currency`
3. Else:
   - return existing missing/invalid request error behavior

### Determinism notes

- Explicit request value always wins.
- Blank and whitespace-only values are treated as omitted.
- Resolved value remains visible in response `quote_currency`.

## 5. Eligibility and auth rules

Fallback is allowed only when all are true:

- request route is protected and authenticated,
- principal maps to an `ib_account_id`, and
- key kind is `account` or `agent`.

Fallback is not allowed for:

- `legacy` keys,
- `anonymous_demo` keys,
- unauthenticated/public routes.

This preserves least-surprise behavior and avoids silently changing
non-account traffic semantics.

## 6. API behavior changes

### 6.1 `POST /v1/balances`

Current: `quote_currency` required.

Proposed:

- `quote_currency` becomes conditionally required:
  - required unless eligible account-scoped fallback applies.
- response continues returning normalized resolved `quote_currency`.
- unsupported explicit values keep current error mapping.

### 6.2 `POST /v1/balances/bulk`

Same behavior as single balances.

### 6.3 `POST /v1/defi-positions/search` (future)

When SPEC-024 route exists and is protected:

- same precedence and eligibility rules apply,
- omitted/blank request `quote_currency` resolves from account default,
- resolved value must remain surfaced in response payload.

Known-position and account-selector families use the same resolution path.

## 7. OpenAPI and contract changes (implementation-time)

When implemented:

- update request schemas to represent conditional quote-currency requirement
  for account-owned protected calls,
- keep value enum unchanged (`USD`, `MXN`, `USDC`, `BTC`),
- document fallback semantics and explicit precedence,
- update `CONTRACTS.md` request field tables and examples,
- ensure examples include:
  - explicit quote_currency,
  - omitted quote_currency with account fallback,
  - omitted quote_currency with ineligible key returning validation error.

## 8. Implementation plan

### Phase 1 - storage

- Add migration introducing `ib_account.default_quote_currency` with strict
  allowlist constraint and `USD` default.

### Phase 2 - auth/principal context

- Extend API-key lookup/projection to include resolved account default quote
  currency when `ib_account_id` exists.
- Carry field in request principal extension so route handlers avoid additional
  per-request DB lookup.

### Phase 3 - request DTO and route resolution

- Make incoming `quote_currency` optional at DTO parse boundary for protected
  operations.
- Resolve effective quote currency in route layer using precedence rules.
- Keep application command/domain validation unchanged and strict.

### Phase 4 - docs and compatibility

- Update OpenAPI generation assertions and examples.
- Update `CONTRACTS.md` and append `HISTORY.md` entry.

## 9. Compatibility and rollout

- Backward compatible for existing clients that already send
  `quote_currency`.
- Behavior only relaxes input requirements for eligible account-owned
  protected callers.
- Non-eligible callers preserve prior strict requirement.

Recommended rollout sequence:

1. deploy migration,
2. deploy runtime fallback logic,
3. publish contract/docs updates,
4. run smoke checks for both explicit and omitted flows.

## 10. Test plan

### Route-level acceptance tests

For both `/v1/balances` and `/v1/balances/bulk`:

- account key + omitted `quote_currency` => uses account default
- account key + blank `quote_currency` => uses account default
- account key + explicit `quote_currency` => explicit wins
- agent key + omitted `quote_currency` => uses owning account default
- legacy key + omitted `quote_currency` => validation error
- anonymous_demo key + omitted `quote_currency` => validation error
- unsupported explicit value => existing unsupported quote error

### Repository/auth tests

- account lookup projects `default_quote_currency` for account and agent keys
- lookup remains `None` for non-account key kinds
- DB constraint rejects unsupported default values

### Contract/OpenAPI tests

- required-field assertions updated for conditional quote requirement
- examples and descriptions reflect fallback precedence and eligibility

## 11. Risks and mitigations

- Risk: accidental fallback for non-account keys.
  - Mitigation: explicit key-kind and `ib_account_id` gating tests.
- Risk: docs drift from runtime precedence.
  - Mitigation: contract/OpenAPI regression assertions for required fields and
    examples.
- Risk: hidden configuration corruption.
  - Mitigation: DB allowlist constraint plus startup/health smoke coverage.

## 12. Acceptance criteria

This spec is ready to accept when:

- migration exists and is reversible via standard forward-only strategy,
- protected balance routes implement precedence exactly as specified,
- ineligible callers still receive strict validation errors when omitting
  `quote_currency`,
- contract/OpenAPI/HISTORY are updated in the same change,
- regression tests cover all eligibility and precedence branches.
