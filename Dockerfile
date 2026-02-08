FROM rust:1-slim-bookworm as builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY . .

ENV SQLX_OFFLINE=true

RUN cargo build --release --bin beetroot

FROM debian:bookworm-slim

WORKDIR /app

RUN apt-get update && apt-get install -y libssl3 ca-certificates sqlite3 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/beetroot /app/beetroot

COPY assets /app/assets

RUN mkdir -p /app/data

ENV DATABASE_URL="sqlite://data/beetroot.db"

CMD ["./beetroot"]