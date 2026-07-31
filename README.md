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

Run Sempre from any directory. Normal commands use protected machine-wide
storage and request administrator access through native UAC on Windows or
`sudo` on Unix when required.

```text
sempre core install sing-box
sempre core use sing-box@stable
sempre subscription set https://example.com/sing-box.json
sempre service install
sempre status
```

Use a local configuration instead of a subscription:

```text
sempre core install sing-box@1.13.15
sempre core use sing-box@1.13.15
sempre config import ./config.json
sempre service install
```

Running Sempre without arguments opens an interactive menu.

To keep both the binary and its data in one movable directory, explicitly
enable portable mode:

```text
sempre portable enable
sempre status
```

The marker command creates `.sempre-portable` beside the executable. You can
also select a mode for one invocation with `--portable` or `--system`;
`--system` overrides an existing portable marker.

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

Installing a version never changes the selected core. Run `core use` after the
first install; this is allowed before a configuration exists. The next
`subscription set` or `config import` validates the configuration with that
selection and activates it. A channel update is validated against the current
configuration before the channel advances.

`core remove` removes the concrete version directory and every channel alias
that points to it. Removal fails while the version is selected, active, or
retained as the one automatic rollback deployment.

`sempre run --core` temporarily runs an installed version without changing the
service selection.

## Configuration

```text
sempre subscription set <https-url>
sempre subscription update
sempre subscription schedule 24h
sempre subscription schedule off
sempre subscription status
sempre subscription clear
sempre subscription set ""
sempre config import <file>
sempre update
```

Both clear forms remove the subscription URL and its check history while
retaining the active configuration.

The top-level `update` command updates only the subscription. Core channels
are updated explicitly with `core update`.

Subscription candidates are limited to 32 MiB, downloaded only through HTTPS,
validated by the selected core, and stored by content hash. A failed download,
validation, resolve, or startup leaves the last known good deployment
available. Sempre retains the current configuration and at most one
configuration needed for automatic rollback; unreferenced configuration
objects are collected after a deployment becomes healthy or rolls back.
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
native system service manager, enables it, and starts it. It also copies Sempre
to a protected system executable directory, so the original download can be
moved or deleted afterwards. `service uninstall` retains the installed binary
and system data.

Portable mode can explicitly deploy prepared offline assets to the system
service:

| Command | Replaces | Preserves |
| --- | --- | --- |
| `service deploy bin` | Sempre service executable and service registration | Core, state, configurations, logs, runtime |
| `service deploy core` | Managed core/version directories from portable mode | Extra system core versions, state, configurations, logs, runtime |
| `service deploy data` | State, subscription metadata, and referenced configurations | Sempre binary, cores, logs, runtime |
| `service deploy all` | Sempre binary, exact managed-core snapshot, state, and referenced configurations | Logs and runtime |

`service deploy` is available only in portable mode and requires an installed
system service. A data-only deployment first verifies that every core version
referenced by the portable state already exists in system storage. `all`
removes system core versions that are not in the portable snapshot; `core`
intentionally keeps them.

Portable `service install` is equivalent to deploying `all`, repairing the
native service registration, and starting the service. An existing meaningful
system state is summarized and requires confirmation before `data`, `all`, or
portable installation replaces it; use `--yes` for unattended deployment. An
initialized but otherwise empty system `state.json` is replaceable without a
prompt. Deployments stage files on the target volume, stop the service only
after staging succeeds, and restore both files and prior service state if
activation fails.

Sempre supervises the core on every platform. Unexpected exits use bounded
exponential backoff. Unix process groups and Windows Job Objects ensure child
processes are cleaned up when the service stops. Portable foreground runs and
the system daemon share one machine-wide instance lock, so they cannot start
two managed sing-box processes at the same time.

Windows elevation uses the native `runas` API. Sempre does not invoke
PowerShell. Linux and macOS use `sudo`. Help, version, portable marker
management, and `service status` do not require elevation. The portable
Windows menu requests elevation once at entry. The portable Unix menu remains
unprivileged and requests `sudo` only for foreground run and service actions.

## Diagnostics

```text
sempre status
sempre logs
sempre logs --follow
sempre doctor
sempre version
```

Logs rotate at 10 MiB with three backups. Sempre does not assume a Clash API
port, TUN interface name, or the presence of another proxy product. `status`
cross-checks the recorded PID with the operating system and the shared instance
lock, so interrupted or forcibly terminated processes are reported as stale.
`doctor` checks files, configuration validation, process consistency, and the
native service manager; an uninstalled service is informational rather than a
broken service executable.

## Data Layout

System mode is the default:

| Platform | Executable | Data | Logs | Runtime |
| --- | --- | --- | --- | --- |
| Windows | `%ProgramFiles%\Sempre\sempre.exe` | `%ProgramData%\Sempre` | `%ProgramData%\Sempre\logs` | `%ProgramData%\Sempre\run` |
| Linux | `/usr/local/libexec/sempre/sempre` | `/var/lib/sempre` | `/var/log/sempre` | `/run/sempre` |
| macOS | `/Library/Application Support/Sempre/bin/sempre` | `/Library/Application Support/Sempre/data` | `/Library/Logs/Sempre` | `/var/run/sempre` |

Portable mode keeps the following structure beside the executable:

```text
sempre.exe
.sempre-portable
.sempre/
|-- state.json
|-- cores/
|   `-- sing-box/<version>/
|-- configs/
|   `-- sing-box/<sha256>.json
|-- logs/
`-- run/
```

The system service always runs the protected system executable with
`--system daemon`, even when installation was initiated from portable mode.

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
