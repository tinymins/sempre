# Sempre Development

Sempre has three development processes: the Rust control plane, the React
control UI, and the public website. The root commands install and run all three
without requiring a system installation of Sempre.

## Prerequisites

- Rust 1.95 or newer
- Bun 1.3.14

Fetch the Rust workspace, install Cargo Watch, and install the locked frontend
dependencies:

```sh
bun bootstrap
```

The command is safe to run again after switching branches or updating a lock
file. Each frontend retains its own lockfile.

## Start Everything

From the repository root, run:

```sh
bun start
```

The command keeps all output in one terminal and prefixes each line with the
process name.

| Process | Address | Reload behavior |
| --- | --- | --- |
| `dev:api` | `http://127.0.0.1:33212` | Cargo Watch rebuilds and restarts the Rust daemon |
| `dev:ui` | `http://127.0.0.1:5173` | Vite HMR |
| `dev:site` | `http://127.0.0.1:4174` | Vite HMR |

Open the control UI address and leave its login address unchanged. The Vite
server proxies `/api` to the development API, so the initial empty password is
accepted as a same-origin login.

Press Ctrl+C once to stop all three processes. The ports are strict: startup
fails instead of silently selecting another port when one is already in use.

## Rust Reloading

Cargo Watch monitors the workspace crates and manifests. A change triggers an
incremental `sempre-client` build, gracefully stops the previous development
daemon, and starts the new binary. A compile error leaves the watcher running;
the two Vite servers remain available while it waits for the next edit.

Cargo Watch is installed by `bun bootstrap`. Build outputs remain in the normal
Rust target directory.

For IDE breakpoints, launch the `sempre-client` package with these arguments and
`rust/` as its working directory:

```text
daemon --development-root ../.cache/sempre-dev/runtime --listen 127.0.0.1:33212
```

Stop `dev:api` first so only one process owns port `33212`; the two Vite
processes can continue running.

## Isolated State

The development API stores its state, downloaded cores, generated
configurations, runtime files, and logs below `.cache/sempre-dev/runtime`.
This directory persists across rebuilds and `bun start` sessions, but is
ignored by Git. Remove that exact directory when a fresh development state is
required.

Development uses port `33212` instead of the production default `33211` and a
disabled system-service controller. It cannot install, stop, or restart the
host's Sempre service. Core and subscription APIs are real and use the isolated
state, but proxy runtime modes such as Linux TProxy can still require
administrator privileges. Validate those behaviors with a portable or system
installation rather than the development server.

## Individual Processes

Run only one component when needed:

```sh
bun run dev:api
bun run dev:ui
bun run dev:site
```

The multi-user, core-free server is independent of the local daemon:

```sh
bun run server
```

The control UI expects `dev:api` to listen on port `33212`. The website is
independent of both backends.

## Validation

Run the repository checks before submitting a change:

```sh
bun run rust:lint
bun run rust:test
bun run lint
bun run tsc
bun run test
```

Release builds and platform artifacts remain documented in the
[`README.md`](README.md#build).
