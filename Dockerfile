# ------------------------------------------------------------------------------
# Cargo Build Stage
# ------------------------------------------------------------------------------

FROM rust:1.53.0 AS builder

RUN apt-get update && apt-get install -y libssl-dev gcc cmake

# RUN rustup default nightly

WORKDIR /app

COPY Cargo.lock Cargo.lock
COPY Cargo.toml Cargo.toml

RUN mkdir src/
RUN mkdir src/bin
RUN touch src/lib.rs
RUN echo "fn main() {println!(\"if you see this, the build broke\")}" > src/bin/main.rs

RUN cargo build --release 

RUN cargo clean -p registrar
RUN rm -rf target/release/.fingerprint/registrar*

COPY . .

RUN cargo build --release

# ------------------------------------------------------------------------------
# Final Stage
# ------------------------------------------------------------------------------

FROM debian:buster-slim

RUN apt-get update && apt-get install -y libssl-dev ca-certificates
RUN update-ca-certificates --fresh

COPY --from=builder /app/target/release/registrar /usr/local/bin

CMD ["/usr/local/bin/registrar"]