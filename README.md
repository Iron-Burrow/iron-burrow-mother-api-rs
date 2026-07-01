---

status: active
owner: iron-burrow
last_reviewed: 2026-07-01
agent_edit_policy: update_when_relevant
---------------------------------------

# Iron Burrow Mother API

Public Beta API boundary for **Iron Burrow**.

Iron Burrow is a source-aware blockchain intelligence system built to make on-chain data easier for humans, applications, and agents to inspect without guessing.

The **Mother API** is the public HTTP surface of the burrow. For the Beta release, it exposes a deliberately small API surface:

1. balance lookups;
2. ERC-20 transfer search.

Everything else is intentionally out of scope for this README.

No hackathon demo routes.
No FIFA routes.
No prediction endpoints.
No broad public catalog explorer.
No experimental surface.

Small surface. Real data. API-key protected.

---

## Production API

```bash
export IB_API="https://api.ironburrow.com"
export IB_API_KEY="replace-with-issued-beta-key"
```

Protected Beta endpoints require an API key.

```bash
-H "Authorization: Bearer $IB_API_KEY"
```

Beta API keys are private credentials. Do not expose them in frontend code, public repositories, logs, screenshots, or client-side agents.

---

## Public Health Check

The health endpoint is public and does not require an API key.

```bash
curl -sS "$IB_API/health" | jq
```

Use this endpoint only to confirm that the Mother API process is reachable.

A healthy response means the HTTP service is alive. It does not imply that every internal data dependency is fully available.

---

## Beta API Surface

| Method | Path                         |    Auth | Purpose                                                      |
| ------ | ---------------------------- | ------: | ------------------------------------------------------------ |
| `POST` | `/v1/balances/bulk`          | API key | Read balances for supported addresses, networks, and assets. |
| `POST` | `/v1/erc20-transfers/search` | API key | Search ERC-20 transfers using the accepted Beta contract.    |
| `GET`  | `/health`                    |  Public | Lightweight service health check.                            |

The OpenAPI contract is the source of truth for exact request and response schemas.

---

## 1. Balance Lookup

Endpoint:

```http
POST /v1/balances/bulk
```

Example:

```bash
curl -sS "$IB_API/v1/balances/bulk" \
  -H "Authorization: Bearer $IB_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "subjects": [
      {
        "network_slug": "eth-mainnet",
        "address": "0x0000000000000000000000000000000000000000"
      }
    ],
    "assets": [
      "ethereum",
      "usdc"
    ],
    "quote_currency": "USD"
  }' | jq
```

Use this endpoint when a caller needs a deterministic balance response for supported networks and assets.

Expected use cases:

* treasury balance checks;
* compliance review workflows;
* internal reporting;
* API consumers that need structured on-chain balance data.

The response should be treated as a structured data result, not as a natural-language answer. Applications and agents should inspect the returned fields, timestamps, evidence, and error states before presenting conclusions to users.

---

## 2. ERC-20 Transfer Search

Endpoint:

```http
POST /v1/erc20-transfers/search
```

Example:

```bash
curl -sS "$IB_API/v1/erc20-transfers/search" \
  -H "Authorization: Bearer $IB_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "network_slug": "eth-mainnet",
    "address": "0x0000000000000000000000000000000000000000",
    "from_block": "latest-safe-start",
    "to_block": "latest-safe-end"
  }' | jq
```

Use this endpoint when a caller needs ERC-20 transfer activity from the Beta-supported extraction contract.

Expected use cases:

* compliance-oriented transfer review;
* address activity inspection;
* customer or counterparty investigation;
* deterministic data extraction for downstream reports.

The exact request filters, limits, and response fields are defined by the current OpenAPI contract.

---

## Authentication Behavior

Protected Beta endpoints require a valid API key.

Typical authentication failures include:

* missing API key;
* malformed authorization header;
* unsupported authentication scheme;
* unknown key;
* disabled key;
* revoked key;
* expired key;
* disabled API consumer;
* temporary authentication storage failure.

Clients should not treat all failures as the same class of error. A bad or inactive key is different from a temporary backend availability problem.

---

## What This Repository Is

This repository contains the Mother API service: the stable public HTTP boundary for selected Iron Burrow capabilities.

The Mother API is responsible for:

* accepting public Beta requests;
* validating request contracts;
* enforcing API-key access;
* returning structured JSON responses;
* hiding internal service topology from external consumers.

The Mother API is not the whole Iron Burrow system. It is the public door.

---

## What This Repository Is Not

This repository is not:

* a hackathon demo app;
* a FIFA prediction service;
* a betting or prediction-market API;
* a frontend;
* a wallet;
* a custody system;
* an execution or trading service;
* the full internal burrow topology.

Historical demo surfaces should remain removed from the active Beta API.

---

## Development

Run locally:

```bash
cargo run
```

Basic local checks:

```bash
curl -i http://localhost:3000/health
```

Development checks:

```bash
cargo fmt --check
cargo check
cargo test
```

Postgres-backed checks may require an explicit test database setup:

```bash
make test-db-postgres
```

---

## API Contract

The README is intentionally high level.

For exact request bodies, response bodies, validation rules, limits, and error shapes, use the OpenAPI contract generated by the repository.

The README should explain the Beta story.
The OpenAPI contract should define the machine contract.

---

## Beta Release Principle

The Beta release favors a small, reliable, protected surface over a large exploratory API.

The goal is to give early customers access to useful on-chain data without exposing unstable internal routes or old demo concepts.

For this release, the Mother API should be boring in the best possible way:

* authenticated;
* narrow;
* deterministic;
* source-aware;
* easy to smoke test;
* safe for early customer access.

---

## License

MIT License.

See [`LICENSE`](LICENSE).
