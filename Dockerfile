FROM oven/bun:1-debian as frontend

WORKDIR /app/apps/dashboard

COPY apps/dashboard/package.json apps/dashboard/bun.lock* ./
RUN bun install --frozen-lockfile

COPY apps/dashboard/ ./
RUN bun run build

FROM rust:1.95-slim as builder

WORKDIR /app

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY --from=frontend /app/apps/dashboard/build ./apps/dashboard/build

RUN cargo build --release --features embedded-frontend -p hyperinfer-server

FROM debian:trixie-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/hyperinfer-server /app/hyperinfer-server
COPY --from=builder /app/crates/hyperinfer-server/migrations /app/crates/hyperinfer-server/migrations

EXPOSE 3000

CMD ["./hyperinfer-server"]