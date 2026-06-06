# Iron Burrow Mother API

Public API boundary for **Iron Burrow**.

Iron Burrow is a source-aware blockchain intelligence system built to make crypto data easier for humans, applications, and AI agents to inspect. The **Mother API** is the public interface of the burrow: the stable HTTP surface that exposes selected assets, price signals, chain mappings, health information, and demo-ready prediction snapshots.

The internal burrow has more tunnels than this README needs to reveal. This repository documents the public door: what a judge, builder, frontend, or agent can call today.

Production API:

```bash
export IB_API="https://api.ironburrow.com"
```

---

## ETHMEX 🇲🇽 Hackathon Context

This repository is part of the **Ethereum México x Bitso Hybrid Hackathon — AI, Blockchain & Payments: Build Today, Play Global**.

The hackathon brings together builders working at the intersection of:

* AI
* blockchain
* stablecoins
* payments
* financial apps
* institutional use cases

For this hackathon, Iron Burrow Mother API acts as the public data boundary between the burrow and the outside world.

In practical terms, this means:

* a frontend can ask Mother API for supported assets and price context;
* an AI agent can call deterministic endpoints instead of inventing answers;
* judges can inspect live public endpoints with `curl`;
* the system can expose blockchain and market signals without leaking every internal service, resolver, worker, or database tunnel.

The goal is not to show a giant API surface. The goal is to show a small, working, public, source-aware interface that an AI or financial application could safely build on top of.

---

## Quick Start

Install `jq` if you want readable JSON output.

```bash
export IB_API="https://api.ironburrow.com"
```

Check that the Mother API is alive:

```bash
curl -sS "$IB_API/health" | jq
```

You should see a small response confirming the service is alive, including the service name, mascot, and a happy systems message.

For raw HTTP headers:

```bash
curl -i "$IB_API/health"
```

---

## Public Endpoints

These examples are intentionally judge-friendly. They are not a replacement for strict contract documentation. They are here so you can poke the live burrow and understand what the response means.

---

### 1. Health

```bash
curl -sS "$IB_API/health" | jq
```

Use this to confirm the public API process is running.

Expected interpretation:

* `ok: true` means the Mother API process is alive.
* This endpoint is lightweight and does not require every internal dependency to be healthy.

---

### 2. Status / Dependency Picture

```bash
curl -sS "$IB_API/v1/status" | jq
```

Use this to get a public readiness picture of the burrow.

Important:

`/v1/status` can return `200 OK` even when one dependency is degraded. Do not only inspect the HTTP status code. Look at:

* `ok`
* `checks`
* individual dependency states

This endpoint is meant to help an operator, judge, frontend, or agent understand whether the public API is alive and whether its connected services are behaving.

---

### 3. Asset List

```bash
curl -sS "$IB_API/v1/assets" | jq
```

With an explicit limit:

```bash
curl -sS "$IB_API/v1/assets?limit=10" | jq
```

Compact view:

```bash
curl -sS "$IB_API/v1/assets?limit=20" \
  | jq '.assets[] | {asset_id, symbol, name, price: .price.status}'
```

Expected interpretation:

This endpoint returns the active asset catalog known by Mother API. Each asset can include a price state.

A price state may be:

* `available` — the burrow has a usable price signal;
* `unavailable` — the asset still exists, but price enrichment is missing or temporarily unavailable.

The asset list is useful for frontends, agents, and demos that need to know what the burrow can currently talk about.

---

### 4. Single Asset Detail

Try known asset slugs:

```bash
curl -sS "$IB_API/v1/assets/bitcoin" | jq
```

```bash
curl -sS "$IB_API/v1/assets/ethereum" | jq
```

```bash
curl -sS "$IB_API/v1/assets/usdc" | jq
```

```bash
curl -sS "$IB_API/v1/assets/bitso-mxn" | jq
```

Use MXN as the quote currency:

```bash
curl -sS "$IB_API/v1/assets/bitso-mxn?quoteCurrency=MXN" | jq
```

Compact view:

```bash
curl -sS "$IB_API/v1/assets/ethereum" \
  | jq '{ok, asset, price, chain_maps}'
```

Expected interpretation:

This endpoint returns one asset, its latest price state, and the chain mappings Mother API knows about.

For example, `ethereum` can be represented as a native asset on Ethereum Mainnet, while `usdc` can have token addresses across multiple networks.

This is useful when an AI agent or frontend needs to answer questions like:

> “What is this asset, what networks does it live on, and does the burrow currently have a price for it?”

---

### 5. Asset Detail With Price Enrichments

USD example:

```bash
curl -sS \
  "$IB_API/v1/assets/ethereum?include=priceStats,priceTrend,priceSeries&quoteCurrency=USD&window=24h&granularity=1h" \
  | jq
```

MXN example:

```bash
curl -sS \
  "$IB_API/v1/assets/ethereum?include=priceStats,priceTrend,priceSeries&quoteCurrency=MXN&window=24h&granularity=1h" \
  | jq
```

Expected interpretation:

This is the richer asset detail path. It can include:

* latest price;
* recent price statistics;
* trend information;
* time series data;
* chain mappings.

This endpoint is useful for a frontend or AI assistant that wants one compact asset response instead of calling several endpoints separately.

If one enrichment is unavailable, the base asset response should still be useful.

---

### 6. Strict Price Stats

```bash
curl -sS \
  "$IB_API/v1/assets/ethereum/signal/price-stats?quoteCurrency=USD&window=24h&granularity=1h" \
  | jq
```

Expected interpretation:

This endpoint focuses only on price statistics for the requested asset, quote currency, time window, and granularity.

Use this when you want a strict stats response instead of a full asset detail payload.

---

### 7. Strict Price Trend

```bash
curl -sS \
  "$IB_API/v1/assets/ethereum/signal/price-trend?quoteCurrency=USD&window=24h&granularity=1h" \
  | jq
```

Expected interpretation:

This endpoint focuses only on the trend signal for the requested asset.

Use this when an agent or application wants to reason about recent price direction without parsing the full asset detail response.

---

### 8. Asset Search / Resolve

```bash
curl -sS "$IB_API/v1/resolve?q=usdc" | jq
```

```bash
curl -sS "$IB_API/v1/resolve?q=oro%20de%20ley" | jq
```

```bash
curl -sS "$IB_API/v1/resolve?q=some%20unknown%20thing" | jq
```

Expected interpretation:

This endpoint helps resolve broad search queries into known Mother API resources.

If the query is known, the response can point to a canonical asset path.

If the query is unknown, the response should still be structured instead of forcing the caller into a blind failure.

This is especially useful for AI and frontend flows where users may search by symbol, name, alias, or natural language.

---

## ETHMEX Demo: World Cup Prediction Snapshots

For the hackathon demo, Mother API also exposes a small public prediction surface related to the 2026 FIFA World Cup.

These endpoints are designed for demo excitement: a judge can ask about a winner market or a country-specific market and get a live, source-aware prediction snapshot.

The response is market-implied data, not advice and not a guarantee. Market probabilities can move.

---

### 9. World Cup Winner Prediction

```bash
curl -sS "$IB_API/v1/predictions/fifa-world-cup/winner" | jq
```

Expected interpretation:

This endpoint returns a snapshot of the current World Cup winner market.

Look for:

* `event`
* `odds`
* `team`
* `probability`
* `source`
* `deterministic`
* `captured_at`

The important hackathon idea is that an AI agent does not need to invent the odds. It can ask the burrow for a deterministic, source-aware snapshot.

---

### 10. World Cup Country Prediction

Mexico:

```bash
curl -sS "$IB_API/v1/predictions/fifa-world-cup/mexico" | jq
```

Try other configured countries:

```bash
curl -sS "$IB_API/v1/predictions/fifa-world-cup/argentina" | jq
```

```bash
curl -sS "$IB_API/v1/predictions/fifa-world-cup/france" | jq
```

```bash
curl -sS "$IB_API/v1/predictions/fifa-world-cup/colombia" | jq
```

```bash
curl -sS "$IB_API/v1/predictions/fifa-world-cup/spain" | jq
```

Expected interpretation:

This endpoint returns a country-specific prediction snapshot.

For example, a judge from Mexico, Colombia, Argentina, France, or Spain can call a country route and immediately see a market-implied probability related to that country.

This is where the hackathon theme becomes visible:

* blockchain data source;
* public API;
* deterministic response;
* AI-friendly structure;
* fun live demo.

---

## Why This Matters For AI Agents

AI agents are powerful, but they should not hallucinate financial, market, or blockchain facts.

Iron Burrow Mother API gives agents a smaller and safer job:

1. call a public endpoint;
2. inspect structured JSON;
3. explain the result to the user;
4. cite the source and timestamp when available.

Mother API is not trying to be the whole burrow. It is the public mouth of the burrow.

---

## Development

Run locally:

```bash
cargo run
```

Basic local checks:

```bash
curl -i http://localhost:3000/health
curl -i 'http://localhost:3000/v1/assets?limit=20'
curl -i 'http://localhost:3000/v1/resolve?q=usdc'
```

Development checks:

```bash
cargo fmt --check
cargo check
cargo test
```

---

## Closing Remarks

Iron Burrow is built around a simple belief:

> AI should be able to interact with blockchain systems through deterministic, source-aware, boringly reliable interfaces.

For ETHMEX, this repository is the public API checkpoint of that idea.

Small surface. Real endpoints. Live data. No need to reveal every tunnel in the burrow.

---

## License

MIT License.

See [`LICENCE`](LICENSE).
