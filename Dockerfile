# ------------------------------------------------------------------------------
# Cargo Build Stage
# ------------------------------------------------------------------------------

FROM rust:1.45.0 AS builder

RUN apt-get update && apt-get install -y librocksdb-dev clang musl-tools libssl-dev

RUN rustup target add x86_64-unknown-linux-musl

WORKDIR /app

COPY Cargo.toml Cargo.toml

RUN mkdir src/
RUN mkdir src/bin
RUN touch src/lib.rs
RUN echo "fn main() {println!(\"if you see this, the build broke\")}" > src/bin/main.rs

RUN cargo build --release 

COPY . .

RUN cargo build --release 

# ------------------------------------------------------------------------------
# Final Stage
# ------------------------------------------------------------------------------

FROM debian:buster-slim

RUN apt-get update && apt-get install -y libssl-dev ca-certificates
RUN update-ca-certificates --fresh

COPY --from=builder /app/target/release/registrar-bot /usr/local/bin

CMD ["/usr/local/bin/registrar-bot"]