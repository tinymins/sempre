# Sempre Rust backend

This workspace contains the shared subscription converter and the multi-user Sempre server.

The converter is a pure boundary: `Profile + SourceSnapshot + Target -> CompileResult`. It does not perform HTTP requests, database access, environment reads, or process management. `sempre-server` owns authentication, PostgreSQL persistence, SSRF-safe source fetching, last-known-good snapshots, artifacts, memberships, and public share manifests. It does not install, start, or supervise proxy cores.

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

Profile updates require `If-Match: "<revision>"`. The manifest exposes a read-only artifact, its SHA-256 digest, ETag, target, profile revision, and the authenticated editor URL.

## Checks

```sh
bun run rust:test
bun run rust:lint
bun run build:server
```
