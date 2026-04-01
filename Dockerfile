# Stage 1: Build
FROM rust:slim-bookworm AS builder

RUN apt-get update && apt-get install -y \
    musl-tools \
    pkg-config \
    build-essential \
    perl \
    && rm -rf /var/lib/apt/lists/*

RUN rustup target add x86_64-unknown-linux-musl

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release --target x86_64-unknown-linux-musl

# Stage 2: Runtime
FROM scratch

COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/scrape_blogger /scrape_blogger

ENTRYPOINT ["/scrape_blogger"]