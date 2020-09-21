FROM rust:1.45.0

WORKDIR /app

COPY . .

RUN  apt-get update && apt-get install -y librocksdb-dev clang

RUN cargo build --bin registrar-bot --release

ENTRYPOINT [ "cargo run --bin registrar-bot --release" ]