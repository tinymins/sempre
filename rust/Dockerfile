FROM rust:1.94-bookworm AS builder
WORKDIR /src
COPY rust ./rust
RUN cargo build --manifest-path=rust/Cargo.toml --release -p sempre-server

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install --no-install-recommends -y ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /src/rust/target/release/sempre-server /usr/local/bin/sempre-server
USER 65532:65532
EXPOSE 8787
ENTRYPOINT ["sempre-server"]
