# Etapa 1: compilar los binarios (servidor + comando de migraciones)
# Misma Debian (bookworm) que la imagen final: "rust:*-slim" a secas ahora
# es trixie y compila contra una glibc más nueva que la de bookworm, y el
# binario no corre ("GLIBC_2.39 not found").
#
# cargo-chef: las dependencias (libsql, reqwest, image, ring...) se compilan
# en una capa aparte que Docker cachea. Si solo cambia código en src/, esa
# capa se reutiliza y solo se recompila lo nuestro (antes se recompilaba
# TODO en cada deploy: esa era la mayor parte de los ~30 min).
FROM rust:1.98-slim-bookworm AS chef
WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef --locked

# 1a) "Receta": la lista de dependencias, sacada de Cargo.toml/Cargo.lock
FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo chef prepare --recipe-path recipe.json

# 1b) Compilar solo las dependencias (capa cacheada) y luego nuestro código
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

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
COPY --from=builder /app/target/release/licencias ./licencias
COPY --from=builder /app/target/release/sunat ./sunat
COPY migraciones ./migraciones

RUN mkdir -p uploads/productos

EXPOSE 3000

CMD ["./minimarket-backend"]