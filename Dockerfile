# https://github.com/LukeMathWalker/cargo-chef
# Leveraging the pre-built Docker images with cargo-chef and the Rust toolchain
FROM lukemathwalker/cargo-chef:latest-rust-1.56.0 AS chef
WORKDIR app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies - this is the caching Docker layer!
RUN apt-get update && apt-get install -y \
	lld pkg-config openssl libssl-dev gcc g++ clang cmake
RUN cargo chef cook --release --recipe-path recipe.json
# Build application
COPY . .
RUN cargo build --release --bin registrar

# We do not need the Rust toolchain to run the binary!
FROM debian:buster-slim AS runtime
WORKDIR app
COPY --from=builder /app/target/release/registrar /usr/local/bin
RUN apt-get update && apt-get install -y \
	openssl ca-certificates
RUN update-ca-certificates --fresh
ENTRYPOINT ["/usr/local/bin/registrar"]
