# Sempre

Sempre is a cross-platform lifecycle manager for proxy cores.

It installs and switches core versions, validates and updates configuration,
registers a native system service, and keeps the selected core running. The
first supported core is [sing-box](https://github.com/SagerNet/sing-box).

> Any core. Always current. Always running.

Sempre is an independent community project. It is not affiliated with
SagerNet, Project X, MetaCubeX, or their respective projects.

## Why

Sempre replaces platform-specific wrapper scripts and third-party service
hosts with one Go binary:

```text
Windows SCM / systemd / launchd
                |
          sempre daemon
                |
      selected core@version
```

Windows service support is implemented directly with the Windows SCM API.
Sempre does not download, bundle, or invoke NSSM or PowerShell.

## Quick Start

Place the Sempre binary in its permanent directory before installing the
service. Sempre stores all of its data beside the executable in `.sempre/`.

```text
sempre core install sing-box
sempre subscription set https://example.com/sing-box.json
sempre service install
sempre status
```

Use a local configuration instead of a subscription:

```text
sempre core install sing-box
sempre config import ./config.json
sempre service install
```

Running Sempre without arguments opens an interactive menu.

## Downloads

Canonical releases are published at
[github.com/sempre-lab/sempre/releases](https://github.com/sempre-lab/sempre/releases).
The latest binaries have stable asset URLs:

| Platform | amd64 | arm64 |
| --- | --- | --- |
| Windows | [Download](https://github.com/sempre-lab/sempre/releases/latest/download/sempre-windows-amd64.exe) | [Download](https://github.com/sempre-lab/sempre/releases/latest/download/sempre-windows-arm64.exe) |
| Linux | [Download](https://github.com/sempre-lab/sempre/releases/latest/download/sempre-linux-amd64) | [Download](https://github.com/sempre-lab/sempre/releases/latest/download/sempre-linux-arm64) |
| macOS | [Download](https://github.com/sempre-lab/sempre/releases/latest/download/sempre-darwin-amd64) | [Download](https://github.com/sempre-lab/sempre/releases/latest/download/sempre-darwin-arm64) |

Release checksums are available from
[`SHA256SUMS`](https://github.com/sempre-lab/sempre/releases/latest/download/SHA256SUMS).

## Core Versions

Sempre treats mutable channels and exact versions differently:

```text
sempre core install sing-box@stable
sempre core install sing-box@1.13.15
sempre core list
sempre core use sing-box@stable
sempre core use sing-box@1.13.15
sempre run --core sing-box@1.13.15
sempre core update sing-box@stable
sempre core remove sing-box@1.13.15
```

An exact install is retained until explicitly removed. A channel is a weak
reference to a concrete version. When `stable` advances, its previous version
is removed only when no exact install, active deployment, rollback deployment,
or other channel still references it.

`sempre run --core` temporarily runs an installed version without changing the
service selection.

## Configuration

```text
sempre subscription set <https-url>
sempre subscription update
sempre subscription schedule 24h
sempre subscription schedule off
sempre subscription status
sempre config import <file>
sempre update
```

The top-level `update` command updates only the subscription. Core channels
are updated explicitly with `core update`.

Subscription candidates are limited to 32 MiB, downloaded only through HTTPS,
validated by the selected core, and stored by content hash. A failed download,
validation, or startup leaves the last known good deployment available.
Subscription URLs are stored with restricted permissions and are redacted from
normal output and logs.

Automatic subscription checks run every 24 hours by default. The interval is
configurable with a minimum of five minutes. A changed scheduled configuration
is applied automatically. Interactive changes ask before restarting a running
service; use `--yes` to restart without prompting or `--no-restart` to leave the
change pending.

## Services

```text
sempre service install
sempre service uninstall
sempre service start
sempre service stop
sempre service restart
sempre service status
```

`service install` validates the selected deployment, registers Sempre with the
native system service manager, enables it, and starts it. `service uninstall`
retains all `.sempre/` data.

Sempre supervises the core on every platform. Unexpected exits use bounded
exponential backoff. Unix process groups and Windows Job Objects ensure child
processes are cleaned up when the service stops.

On Windows, the executable runs as the current user by default. Commands that
control the system service request native UAC elevation only when needed.
Read-only commands do not prompt for elevation.

## Diagnostics

```text
sempre status
sempre logs
sempre logs --follow
sempre doctor
sempre version
```

Logs rotate at 10 MiB with three backups. Sempre does not assume a Clash API
port, TUN interface name, or the presence of another proxy product.

## Data Layout

```text
sempre.exe
.sempre/
|-- state.json
|-- cores/
|   `-- sing-box/<version>/
|-- configs/
|   `-- sing-box/<sha256>.json
|-- logs/
`-- run/
```

The service registration records the executable's absolute path. Moving the
binary after service installation requires running `sempre service install`
again to repair the registration.

## Build

Go 1.25 or newer is required. The build is pure Go and uses `CGO_ENABLED=0`.

```text
go test ./...
go vet ./...
go run ./cmd/build
```

The build command emits Windows, Linux, and macOS binaries for amd64 and arm64,
plus `dist/SHA256SUMS`. Windows resources use an `asInvoker` manifest; UAC is
requested at runtime only for privileged commands.

Sempre itself does not redistribute sing-box. Core releases are downloaded at
runtime from the official GitHub release and verified against the SHA-256
digest supplied by GitHub's release API.

## License

Sempre is licensed under the [BSD 3-Clause License](LICENSE). Downloaded proxy
cores remain subject to their own licenses.
