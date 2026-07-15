FROM rust:1.96-bookworm AS builder
WORKDIR /app
COPY apps/api/Cargo.toml apps/api/Cargo.toml
COPY apps/api/Cargo.lock apps/api/Cargo.lock
COPY apps/api/src apps/api/src
WORKDIR /app/apps/api
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && groupadd --system app \
 && useradd --system --gid app --home-dir /app --shell /usr/sbin/nologin app
WORKDIR /app
COPY --from=builder /app/apps/api/target/release/gmail-helpdesk-api /usr/local/bin/gmail-helpdesk-api
USER app
EXPOSE 8080
CMD ["gmail-helpdesk-api"]
