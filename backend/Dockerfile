# Etapa 1: compilar el binario
FROM rust:1.82-slim AS builder

WORKDIR /app

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release

# Etapa 2: imagen final, liviana, solo con el binario ya compilado
FROM debian:bookworm-slim

WORKDIR /app

RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/minimarket-backend ./minimarket-backend

RUN mkdir -p uploads/productos

EXPOSE 3000

CMD ["./minimarket-backend"]