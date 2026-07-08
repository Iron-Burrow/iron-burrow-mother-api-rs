---
status: active
owner: iron-burrow
last_reviewed: 2026-07-08
agent_edit_policy: update_when_relevant
---

# Iron Burrow Mother API

Public Beta v0.2 API boundary for **Iron Burrow**.

Iron Burrow is a source-aware blockchain intelligence system built to make
on-chain data easier for humans, applications, and agents to inspect without
guessing.

The **Mother API** is the public HTTP surface of the burrow. In private Beta
mode, it exposes a deliberately small API surface:

1. latest balance lookups;
2. bounded ERC-20 transfer search.

Everything else is intentionally out of scope for this README. No hackathon
demo routes, no FIFA routes, no prediction endpoints, no broad public catalog
explorer, and no experimental public sprawl.

Small surface. Real data. API-key protected.

## Production API

```bash
export IB_API="https://api.ironburrow.com"
export IB_API_KEY="replace-with-issued-beta-key"
```

Private Beta `/v1/*` endpoints require an issued API key:

```bash
-H "Authorization: Bearer $IB_API_KEY"
```

Beta API keys are private credentials. Do not expose them in frontend code,
public repositories, logs, screenshots, or client-side agents.

For a customer-facing copy-paste guide, see
[Private Beta API Quickstart](docs/runbooks/private-beta-api-quickstart.md).

The production Beta deployment should run with `PUBLIC_API_SURFACE=beta` and
`ERC20_TRANSFERS_ENABLED=true`. Alpha compatibility mode still exists for the
broader Production Alpha 1 route surface and is not the private Beta v0.2
customer surface.

## Public Health Check

The health endpoint is public and does not require an API key.

```bash
curl -sS "$IB_API/health" | jq
```

Use this endpoint only to confirm that the Mother API process is reachable. A
healthy response means the HTTP service is alive; it does not imply that every
internal data dependency is fully available.

## Beta API Surface

| Method | Path                         | Auth    | Purpose |
| ------ | ---------------------------- | ------- | ------- |
| `GET`  | `/health`                    | Public  | Lightweight process liveness check. |
| `POST` | `/v1/balances`               | API key | Read latest balances for one supported account. |
| `POST` | `/v1/balances/bulk`          | API key | Read latest balances for supported accounts, networks, and assets. |
| `POST` | `/v1/erc20-transfers/search` | API key | Search bounded ERC-20 transfers. |

In Beta mode, known Alpha-only routes return `403 endpoint_disabled`. Truly
unknown routes remain normal `404` responses.

`CONTRACTS.md` and the generated OpenAPI document are the sources of truth for
exact request bodies, response bodies, validation rules, limits, and error
shapes.

## Balance Lookup

Single-account endpoint:

```http
POST /v1/balances
```

Bulk endpoint:

```http
POST /v1/balances/bulk
```

Example bulk request:

```bash
curl -sS "$IB_API/v1/balances/bulk" \
  -H "Authorization: Bearer $IB_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "as_of": {
      "kind": "latest"
    },
    "accounts": [
      {
        "network_slug": "eth-mainnet",
        "address": "0x1234567890abcdef1234567890abcdef1234beef",
        "client_ref": "treasury-main"
      },
      {
        "network_slug": "base-mainnet",
        "address": "0x2222222222222222222222222222222222222222",
        "client_ref": "treasury-base"
      }
    ],
    "quote_currency": "USD",
    "tokens": {
      "asset_slugs": ["ethereum", "usdc"],
      "contract_addresses": []
    }
  }' | jq
```

Use balance endpoints when a caller needs deterministic, structured balance
results for explicitly supported networks, catalog token asset slugs, or
explicit ERC-20 contract addresses. Balance `as_of` supports latest,
timestamp, and block-number requests. Responses are data results, not
natural-language answers; applications and agents should inspect fields,
timestamps, evidence, skipped items, and error states before presenting
conclusions to users.

## ERC-20 Transfer Search

Endpoint:

```http
POST /v1/erc20-transfers/search
```

This endpoint is registered by `ERC20_TRANSFERS_ENABLED=true`, which is
required for the production private Beta deployment.

Example:

```bash
curl -sS "$IB_API/v1/erc20-transfers/search" \
  -H "Authorization: Bearer $IB_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "account": {
      "network_slug": "eth-mainnet",
      "address": "0xabc0000000000000000000000000000000000000",
      "client_ref": "treasury-main"
    },
    "direction": "any",
    "tokens": {
      "asset_slugs": [
        "usdc"
      ],
      "contract_addresses": []
    },
    "window": {
      "from_block": 18600000,
      "to_block": 18600500
    }
  }' | jq
```

Use this endpoint when a caller needs bounded ERC-20 transfer activity for one
Ethereum mainnet address. It does not index transfers, infer native transfers,
or enrich transfer rows with prices.

## Authentication Behavior

Protected Beta endpoints require a valid API key. Missing, malformed,
unsupported, unknown, disabled, revoked, expired, or disabled-consumer
credentials return `401 unauthorized`.

If authentication storage is temporarily unavailable while Mother API checks a
valid-format key, the request returns `503 database_unavailable`. If a valid
key exceeds its configured request limits, the request returns
`429 rate_limited`.

## What This Repository Is

This repository contains the Mother API service: the public HTTP boundary for
selected Iron Burrow capabilities.

The Mother API is responsible for:

- accepting private Beta requests;
- validating public request contracts;
- enforcing API-key access on Beta `/v1/*` routes;
- returning structured JSON responses;
- hiding internal service topology from external consumers.

The Mother API is not the whole Iron Burrow system. It is the public door.

## What This Repository Is Not

This repository is not:

- a hackathon demo app;
- a FIFA prediction service;
- a betting or prediction-market API;
- a frontend;
- a wallet;
- a custody system;
- an execution or trading service;
- the price indexer, event indexer, holder indexer, or read-model scheduler;
- the full internal burrow topology.

Historical demo surfaces should remain removed from the active Beta API.

## Development

Run locally:

```bash
cargo run
```

Basic local check:

```bash
curl -i http://localhost:3000/health
```

Development checks:

```bash
cargo fmt --check
cargo check
cargo test
```

Postgres-backed regression tests intentionally require a disposable test
database:

```bash
make test-db-postgres
```

Production-style migration smoke checks run through the Mother API binary and
Docker image:

```bash
make smoke-db-migrate
```

## API Contract

The README is intentionally high level. It explains the Beta story and gives
human-readable examples.

For the binding public contract, use `CONTRACTS.md`. For machine-readable
schemas and examples, use the generated OpenAPI document.

## Beta Release Principle

Beta v0.2 favors a small, reliable, protected surface over a large exploratory
API. The goal is to give early customers useful on-chain data without exposing
unstable internal routes or old demo concepts.

For this release, the Mother API should be boring in the best possible way:

- authenticated;
- narrow;
- deterministic;
- source-aware;
- easy to smoke test;
- safe for early customer access.

## License

MIT License.

See [`LICENSE`](LICENSE).
