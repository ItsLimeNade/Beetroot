FROM rust:1-slim-bookworm AS builder

WORKDIR /app

RUN apt-get update \
 && apt-get install -y --no-install-recommends pkg-config libssl-dev \
 && rm -rf /var/lib/apt/lists/*

COPY . .

ENV SQLX_OFFLINE=true

RUN cargo build --release --locked --bin bot \
 && cp target/release/bot /usr/local/bin/bot

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates libssl3 \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --user-group --create-home beetroot

WORKDIR /app

COPY --from=builder /usr/local/bin/bot /usr/local/bin/bot
COPY assets /app/assets

RUN mkdir -p /app/data && chown -R beetroot:beetroot /app

ENV DATABASE_URL="sqlite:///app/data/beetroot.db"

USER beetroot

CMD ["bot"]
