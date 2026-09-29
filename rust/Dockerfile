FROM oven/bun:1.3.14 AS web-builder
WORKDIR /src
COPY package.json bun.lock ./
COPY ui/package.json ./ui/package.json
COPY server-ui/package.json ./server-ui/package.json
COPY packages/components/package.json ./packages/components/package.json
COPY packages/subscription-editor/package.json ./packages/subscription-editor/package.json
RUN bun install --frozen-lockfile
COPY server-ui ./server-ui
COPY packages/components ./packages/components
COPY packages/subscription-editor ./packages/subscription-editor
RUN bun --cwd=server-ui run build

FROM rust:1.95-bookworm AS builder
WORKDIR /src
COPY rust ./rust
RUN cargo build --manifest-path=rust/Cargo.toml --release -p sempre-server

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install --no-install-recommends -y ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /src/rust/target/release/sempre-server /usr/local/bin/sempre-server
COPY --from=web-builder /src/server-ui/dist /usr/share/sempre/ui
ENV SEMPRE_WEB_ROOT=/usr/share/sempre/ui
USER 65532:65532
EXPOSE 8787
HEALTHCHECK --interval=10s --timeout=3s --start-period=10s --retries=6 \
    CMD ["curl", "--fail", "--silent", "http://127.0.0.1:8787/api/v1/health"]
ENTRYPOINT ["sempre-server"]
