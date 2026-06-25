FROM rust:1.96-slim AS builder
ARG HTTP_PROXY
ARG HTTPS_PROXY
ARG ALL_PROXY
ARG CARGO_MIRROR_URL=sparse+https://rsproxy.cn/index/
ENV HTTP_PROXY=$HTTP_PROXY HTTPS_PROXY=$HTTPS_PROXY ALL_PROXY=$ALL_PROXY \
    CARGO_HTTP_MULTIPLEXING=false
WORKDIR /workspace/apps/server
RUN printf '[source.crates-io]\nreplace-with = "quicknote-mirror"\n[source.quicknote-mirror]\nregistry = "%s"\n' "$CARGO_MIRROR_URL" > "$CARGO_HOME/config.toml"
COPY apps/server/Cargo.toml apps/server/Cargo.lock* ./
COPY crates /workspace/crates
RUN mkdir src && echo 'fn main() {}' > src/main.rs
RUN cargo fetch --locked
RUN cargo build --release --locked
COPY apps/server .
RUN touch src/main.rs && cargo build --release --locked --offline

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates wget && rm -rf /var/lib/apt/lists/*
COPY --from=builder /workspace/apps/server/target/release/quicknote-server /usr/local/bin/
EXPOSE 3000
CMD ["quicknote-server"]
