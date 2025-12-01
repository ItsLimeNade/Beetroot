FROM rust:1-bookworm AS builder
WORKDIR /app

COPY . .

RUN cargo build --release

FROM debian:bookworm-slim AS runner
WORKDIR /app

RUN apt-get update && \
    apt-get install -y ca-certificates libssl-dev && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/beetroot ./beetroot

COPY --from=builder /app/assets ./assets

CMD ["./beetroot"]