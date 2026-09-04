# Etapa 1: compilar los binarios (servidor + comando de migraciones)
FROM rust:1.98-slim AS builder

WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY schema.sql ./schema.sql

# cargo build --release compila TODOS los targets del crate: el binario
# principal (src/main.rs) y cualquier binario extra en src/bin/*.rs
# (aquí, src/bin/migrar.rs) — no hace falta declarar nada aparte.
RUN cargo build --release

# Etapa 2: imagen final, liviana, solo con los binarios ya compilados
FROM debian:bookworm-slim

WORKDIR /app

RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/minimarket-backend ./minimarket-backend
COPY --from=builder /app/target/release/migrar ./migrar
COPY migraciones ./migraciones

RUN mkdir -p uploads/productos

EXPOSE 3000

CMD ["./minimarket-backend"]