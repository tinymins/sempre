# Sempre Rust backend

This workspace contains the shared subscription converter, the multi-user Sempre server, and the replacement local-client foundations.

The converter is a pure boundary: `Profile + SourceSnapshot + Target -> CompileResult`. It does not perform HTTP requests, database access, environment reads, or process management. `sempre-server` owns authentication, PostgreSQL persistence, SSRF-safe source fetching, last-known-good snapshots, artifacts, memberships, and public share manifests. It does not install, start, or supervise proxy cores.

`sempre-state` owns the client's filesystem layout, validated persistent state, atomic writes, and single-instance leases. The project does not migrate state from pre-Rust releases.

`sempre-client` produces the release `sempre` binary and owns the CLI, local API, native service lifecycle, core supervision, subscription scheduling, tunnels, gateway services, and transparent-network orchestration.

The shared local/server conversion boundary is documented
in [`docs/rust-subscription-integration.md`](../docs/rust-subscription-integration.md).

## Local server

Copy `.env.example` to a private environment file, create the PostgreSQL database, export the variables, then run from the repository root:

```sh
bun run server
```

The process applies embedded SQL migrations before listening. `SEMPRE_PUBLIC_URL` must be the externally reachable HTTP(S) base URL and should end with `/`.

For a container deployment, set `POSTGRES_PASSWORD` and `SEMPRE_PUBLIC_URL`, then run:

```sh
docker compose --env-file .env -f rust/compose.yml up --build
```

## API flow

1. Register or log in under `/api/v1/auth/*` and use the opaque bearer token.
2. Create a profile under `/api/v1/profiles`.
3. Compile each required target with `POST /api/v1/profiles/{id}/compile`.
4. Create a share with `POST /api/v1/profiles/{id}/shares`.
5. Give clients the returned manifest URL. Public tokens are independent from edit sessions and only their SHA-256 hashes are stored.

Reusable nodes are managed under `/api/v1/custom-nodes`, per-profile artifact
statistics under `/api/v1/profiles/{id}/stats`, and Clash rule-provider
compatibility under `/api/proxy/sing-box/convert/rule[/12|/13]`. Source fetches
honor `cache_ttl_minutes`; sources using `fetch_mode=domestic-direct` use
`DIRECT_PROXY_URL` when the administrator configures it.

Profile updates require `If-Match: "<revision>"`. The manifest exposes a read-only artifact, its SHA-256 digest, ETag, target, profile revision, and the authenticated editor URL.

## Checks

```sh
bun run rust:test
bun run rust:lint
bun run build:server
```
