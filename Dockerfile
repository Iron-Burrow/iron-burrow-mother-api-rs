FROM rust:1.95-bookworm AS builder

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
COPY reference-data ./reference-data

RUN cargo build --release --locked --bin mother-api

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates wget \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --system --uid 10001 --home /nonexistent --shell /usr/sbin/nologin mother-api

WORKDIR /app

COPY --from=builder /app/target/release/mother-api /usr/local/bin/mother-api

ENV APP_ENV=production
ENV HTTP_HOST=0.0.0.0
ENV HTTP_PORT=3000

EXPOSE 3000

USER mother-api

CMD ["mother-api", "serve"]
