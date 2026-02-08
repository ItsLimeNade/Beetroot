# Builder
FROM rust:1-slim-bookworm as builder

# Install build dependencies
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy manifest files to cache dependencies
COPY Cargo.toml Cargo.lock ./
COPY crates/bot/Cargo.toml crates/bot/
COPY crates/database/Cargo.toml crates/database/
COPY crates/macros/Cargo.toml crates/macros/

# Create dummy source files to trigger dependency build
RUN mkdir -p crates/bot/src && echo "fn main() {}" > crates/bot/src/main.rs
RUN mkdir -p crates/database/src && echo "pub fn dummy() {}" > crates/database/src/lib.rs
RUN mkdir -p crates/macros/src && echo "pub fn dummy() {}" > crates/macros/src/lib.rs

# Build dependencies
RUN cargo build --release

RUN rm -rf crates/bot/src crates/database/src crates/macros/src target/release/deps/bot* target/release/deps/database* target/release/deps/macros*

COPY crates crates

COPY .sqlx .sqlx/
ENV SQLX_OFFLINE=true

ARG CARGO_BUILD_JOBS

# Build the actual application
RUN cargo build --release --bin bot

# Runtime
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y libssl3 ca-certificates sqlite3 && rm -rf /var/lib/apt/lists/*

# Copy the binary from the builder stage
COPY --from=builder /app/target/release/bot /app/bot

# Create data directory
RUN mkdir -p /app/data

# Copy assets
COPY assets /app/assets

# Set environment
ENV DATABASE_URL="sqlite://data/beetroot.db"

# Run the bot
CMD ["./bot"]