FROM rust:1.96-bookworm AS builder
WORKDIR /app
COPY apps/api/Cargo.toml apps/api/Cargo.toml
COPY apps/api/src apps/api/src
WORKDIR /app/apps/api
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/apps/api/target/release/gmail-helpdesk-api /usr/local/bin/gmail-helpdesk-api
EXPOSE 8080
CMD ["gmail-helpdesk-api"]

