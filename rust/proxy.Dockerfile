FROM ghcr.io/sagernet/sing-box:v1.14.3@sha256:80c639ba467885b28511fb7c980565c2d30a8752f778e7bc8abdbc91fbfd95ec AS singbox
FROM ghcr.io/xtls/xray-core:26.3.27@sha256:592ec4d11f656db95598d01e76dbcc6e002d67360b96a5436500a938230f52c7 AS xray

FROM rust:1.95-bookworm AS builder
WORKDIR /src
COPY rust ./rust
RUN cargo build --manifest-path=rust/Cargo.toml --release -p sempre-proxy

FROM debian:bookworm-slim
RUN apt-get update && apt-get install --no-install-recommends -y ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
COPY --from=singbox /usr/local/bin/sing-box /usr/local/bin/sing-box
COPY --from=xray /usr/local/bin/xray /usr/local/bin/xray
COPY --from=builder /src/rust/target/release/sempre-proxy /usr/local/bin/
USER 65532:65532
EXPOSE 8790
HEALTHCHECK --interval=10s --timeout=3s --start-period=10s --retries=6 \
    CMD ["curl", "--fail", "--silent", "http://127.0.0.1:8790/health"]
ENTRYPOINT ["sempre-proxy"]
