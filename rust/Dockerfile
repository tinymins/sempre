FROM oven/bun:1.3.14 AS web-builder
WORKDIR /src/ui
COPY ui/package.json ui/bun.lock ./
RUN bun install --frozen-lockfile
COPY ui ./
RUN bun run build

FROM rust:1.94-bookworm AS builder
WORKDIR /src
COPY rust ./rust
RUN cargo build --manifest-path=rust/Cargo.toml --release -p sempre-server

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install --no-install-recommends -y ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /src/rust/target/release/sempre-server /usr/local/bin/sempre-server
COPY --from=web-builder /src/ui/dist /usr/share/sempre/ui
ENV SEMPRE_WEB_ROOT=/usr/share/sempre/ui
USER 65532:65532
EXPOSE 8787
ENTRYPOINT ["sempre-server"]
