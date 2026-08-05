# Sempre Development

Sempre has three development processes: the Go control plane, the React
control UI, and the public website. The root commands install and run all three
without requiring a system installation of Sempre.

## Prerequisites

- Go 1.25 or newer
- Bun 1.3.14

Install the locked frontend dependencies, download the Go module graph, and
prepare the project-pinned Air executable:

```sh
bun bootstrap
```

The command is safe to run again after switching branches or updating a lock
file. It uses each frontend project's existing lockfile and does not create a
shared workspace lockfile.

## Start Everything

From the repository root, run:

```sh
bun start
```

The command keeps all output in one terminal and prefixes each line with the
process name.

| Process | Address | Reload behavior |
| --- | --- | --- |
| `dev:api` | `http://127.0.0.1:33212` | Air rebuilds and restarts the Go process |
| `dev:ui` | `http://127.0.0.1:5173` | Vite HMR |
| `dev:site` | `http://127.0.0.1:4174` | Vite HMR |

Open the control UI address and leave its login address unchanged. The Vite
server proxies `/api` to the development API, so the initial empty password is
accepted as a same-origin login.

Press Ctrl+C once to stop all three processes. The ports are strict: startup
fails instead of silently selecting another port when one is already in use.

## Go Reloading

Go does not update a running process in place. Air watches non-test Go files
under `cmd/` and `internal/`, plus `go.mod` and `go.sum`. A change triggers an
incremental build of `cmd/develop`, gracefully stops the previous process, and
starts the new binary. A compile error leaves the watcher running so the API
returns after the next successful edit; the two Vite servers remain available.

Air is pinned as a Go tool in `go.mod`. No global Air installation is needed.
Temporary binaries are written below `.cache/sempre-dev/build` and removed
when the watcher exits.

For IDE breakpoints, launch the Go package `./cmd/develop` with the repository
root as its working directory. Stop `dev:api` first so only one process owns
port `33212`; the two Vite processes can continue running.

## Isolated State

The development API stores its state, downloaded cores, generated
configurations, runtime files, and logs below `.cache/sempre-dev/runtime`.
This directory persists across rebuilds and `bun start` sessions, but is
ignored by Git. Delete that exact directory when a fresh development state is
required.

Development uses port `33212` instead of the production default `33211` and a
disabled system-service controller. It cannot install, stop, or restart the
host's Sempre service. Core and subscription APIs are real and use the
isolated state, but proxy runtime modes such as Linux TProxy can still require
administrator privileges. Validate those behaviors with a portable or system
installation rather than the development server.

## Individual Processes

Run only one component when needed:

```sh
bun run dev:api
bun run dev:ui
bun run dev:site
```

The control UI expects `dev:api` to be listening on port `33212`. The website
is independent of the API and control UI.

## Validation

Run the repository checks before submitting a change:

```sh
bun run lint
bun run tsc
bun run test
go test ./...
go test -race ./...
go vet ./...
```

Release builds and platform artifacts remain documented in the
[`README.md`](README.md#build).
