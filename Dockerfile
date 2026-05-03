# syntax=docker/dockerfile:1.7

FROM rust:1-slim-bookworm AS builder

WORKDIR /app

COPY . .

# sqlx::query! macros are checked against pre-generated `.sqlx/`
# JSON instead of a live DB at compile time.
ENV SQLX_OFFLINE=true

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --locked --bin bot --bin beetroot-dashboard \
 && cp target/release/bot /usr/local/bin/bot \
 && cp target/release/beetroot-dashboard /usr/local/bin/beetroot-dashboard

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /usr/local/bin/bot /usr/local/bin/bot
COPY --from=builder /usr/local/bin/beetroot-dashboard /usr/local/bin/beetroot-dashboard
COPY assets /app/assets

RUN mkdir -p /app/data

ENV DATABASE_URL="sqlite:///app/data/beetroot.db"

CMD ["bot"]
