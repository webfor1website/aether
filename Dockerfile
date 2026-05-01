# Stage 1 — builder
FROM rust:1.75-slim as builder
WORKDIR /app
COPY . .
RUN cargo build --release --bin aether-server

# Stage 2 — runtime
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/aether-server /usr/local/bin/aether-server
EXPOSE 3000
ENV PORT=3000
CMD ["aether-server"]
