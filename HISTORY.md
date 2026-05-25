I need your help. I know it's sunday, but what ever, iron burrow is fun is not work.

Here the thing, the TypyScript app in iron-burrow-api was good, but it was originilally made as api-gateway. The code is messy, it goes anywhere. We want to follow the trend, rebuild in rust, but this time is not rebuild, is build it instead in rust. I want to pay the price for that safety and stability. Iron and Rust work together.

I'm working on a replacement instead iron burrow mother api rs. a repository that starts from scratch, fresh. 

## 2026-05-24

The first Rust shape is now in place: a tiny Axum Mother API with only `/health`
and `/v1/status`, plus Docker and Compose files that preserve the production
container/network assumptions. The old TypeScript app stays as reference, but no
gateway routes, indexers, database checks, auth, or price logic were ported.

## 2026-05-25

Added `GET /v1/assets` for listing active global assets with a default limit of
100 and a clamped maximum of 1000.
