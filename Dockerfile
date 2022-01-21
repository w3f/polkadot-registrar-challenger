# https://github.com/LukeMathWalker/cargo-chef
# Leveraging the pre-built Docker images with 
# cargo-chef and the Rust toolchain
FROM lukemathwalker/cargo-chef:latest-rust-1.58.1 AS chef
WORKDIR app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder 
RUN apt-get update && apt-get install -y libssl-dev gcc cmake
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --recipe-path recipe.json
# Build application
COPY . .
RUN cargo build --release

# We do not need the Rust toolchain to run the binary!
FROM debian:buster-slim AS runtime
RUN apt-get update && apt-get install -y libssl-dev ca-certificates sqlite3
RUN update-ca-certificates --fresh
COPY --from=builder /app/target/release/registrar /usr/local/bin
ENTRYPOINT ["/usr/local/bin/registrar"]
