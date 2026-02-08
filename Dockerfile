FROM rust:1-slim-bookworm as builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY crates/bot/Cargo.toml crates/bot/
COPY crates/database/Cargo.toml crates/database/
COPY crates/macros/Cargo.toml crates/macros/

RUN mkdir -p crates/bot/src && echo "fn main() {}" > crates/bot/src/main.rs
RUN mkdir -p crates/database/src && echo "pub fn dummy() {}" > crates/database/src/lib.rs
RUN mkdir -p crates/macros/src && echo 'extern crate proc_macro; use proc_macro::TokenStream; #[proc_macro] pub fn dummy(_: TokenStream) -> TokenStream { TokenStream::new() }' > crates/macros/src/lib.rs

RUN cargo build --release

RUN rm -rf crates/bot/src crates/database/src crates/macros/src target/release/deps/beetroot* target/release/deps/database* target/release/deps/macros* target/release/beetroot*

COPY crates crates

COPY .sqlx .sqlx/
ENV SQLX_OFFLINE=true

ARG CARGO_BUILD_JOBS
RUN cargo build --release --bin beetroot

FROM debian:bookworm-slim

WORKDIR /app

RUN apt-get update && apt-get install -y libssl3 ca-certificates sqlite3 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/beetroot /app/beetroot

RUN mkdir -p /app/data

COPY assets /app/assets

ENV DATABASE_URL="sqlite://data/beetroot.db"

CMD ["./beetroot"]