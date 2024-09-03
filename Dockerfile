FROM rust:1.80.1-slim AS builder

RUN rustup target add x86_64-unknown-linux-musl && \
    apt-get update && apt-get install -y \
    musl-tools \
    pkg-config \
    libssl-dev \
    build-essential \
    perl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/app

COPY . .

RUN cargo build --release --target x86_64-unknown-linux-musl

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && update-ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/local/bin

COPY --from=builder /usr/src/app/target/x86_64-unknown-linux-musl/release/scrape_blogger /usr/local/bin/scrape_blogger

CMD ["scrape_blogger"]
