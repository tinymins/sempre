# Sempre

Sempre is a cross-platform lifecycle manager for proxy cores.

It installs and switches core versions, validates and updates configuration,
registers a native system service, and keeps the selected core running. The
first supported core is [sing-box](https://github.com/SagerNet/sing-box).

> Any core. Always current. Always running.

Sempre is an independent community project. It is not affiliated with
SagerNet, Project X, MetaCubeX, or their respective projects.

> [!WARNING]
> Sempre is pre-1.0 software that installs a privileged system service and
> manages network proxy processes. Review release notes, keep a working
> recovery path, and test upgrades on a non-critical machine first. Initial
> v0.1 binaries are not code-signed; verify checksums and GitHub attestations
> before running them.

## Why

Sempre replaces platform-specific wrapper scripts and third-party service
hosts with one Go binary and a separately replaceable Web UI:

```text
Browser / CLI
      |
Sempre API on localhost:33211
      |
sempre daemon ---- selected core@version
      |
Windows SCM / systemd / launchd
```

Windows service support is implemented directly with the Windows SCM API.
Sempre does not download, bundle, or invoke NSSM or PowerShell.

## Quick Start

Download and extract the bundle for your platform, then run:

```text
sempre install
```

`install` can be run repeatedly to install or repair Sempre. It copies the
binary and bundled UI to protected system storage, registers the native
service, starts the Web control plane, and opens it in the default browser.
No proxy core or configuration is required at installation time; the service
reports `idle` until one is configured.

Running the binary without arguments, including by double-clicking it, shows
only the current version/status and four actions:

| Action | Result |
| --- | --- |
| Open Web UI | Opens the discovered local control-plane address |
| Install / Repair | Runs the idempotent system installation |
| Uninstall | Keeps configuration by default, with an explicit purge choice |
| Run Portable | Runs the Web control plane and selected core beside the binary |

The launcher contains no settings. Configure everything through the Web UI or
the equivalent CLI commands. A complete CLI setup remains available:

```text
sempre core install sing-box@stable
sempre core use sing-box@stable
sempre subscription set https://example.com/sing-box.json
sempre open
```

To keep both the binary and its data in one movable directory:

```text
sempre --portable portable run
```

`sempre portable enable` creates a persistent `.sempre-portable` marker beside
the executable. `--portable` and `--system` select a mode for one invocation.

## Downloads

Canonical releases are published at
[github.com/tinymins/sempre/releases](https://github.com/tinymins/sempre/releases).
Bundles are recommended because they include the verified official UI:

| Platform | amd64 | arm64 |
| --- | --- | --- |
| Windows | [Bundle](https://github.com/tinymins/sempre/releases/latest/download/sempre-bundle-windows-amd64.zip) | [Bundle](https://github.com/tinymins/sempre/releases/latest/download/sempre-bundle-windows-arm64.zip) |
| Linux | [Bundle](https://github.com/tinymins/sempre/releases/latest/download/sempre-bundle-linux-amd64.zip) | [Bundle](https://github.com/tinymins/sempre/releases/latest/download/sempre-bundle-linux-arm64.zip) |
| macOS | [Bundle](https://github.com/tinymins/sempre/releases/latest/download/sempre-bundle-darwin-amd64.zip) | [Bundle](https://github.com/tinymins/sempre/releases/latest/download/sempre-bundle-darwin-arm64.zip) |

Standalone binaries remain available and download `sempre-ui.zip` from the
matching release during installation:

| Platform | amd64 | arm64 |
| --- | --- | --- |
| Windows | [Download](https://github.com/tinymins/sempre/releases/latest/download/sempre-windows-amd64.exe) | [Download](https://github.com/tinymins/sempre/releases/latest/download/sempre-windows-arm64.exe) |
| Linux | [Download](https://github.com/tinymins/sempre/releases/latest/download/sempre-linux-amd64) | [Download](https://github.com/tinymins/sempre/releases/latest/download/sempre-linux-arm64) |
| macOS | [Download](https://github.com/tinymins/sempre/releases/latest/download/sempre-darwin-amd64) | [Download](https://github.com/tinymins/sempre/releases/latest/download/sempre-darwin-arm64) |

Release checksums are available from
[`SHA256SUMS`](https://github.com/tinymins/sempre/releases/latest/download/SHA256SUMS).
Each target also includes a CycloneDX JSON SBOM. Verify build provenance with:

```text
gh attestation verify <downloaded-binary> --repo tinymins/sempre
```

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

## Web Control Plane

Sempre always serves its versioned API and installed UI while the daemon is
running, including when no core has been selected. The default listener is
`127.0.0.1:33211`; discovery metadata is written beside the installed binary,
so `sempre open` and the launcher do not assume a hard-coded port.

```text
sempre web status --json
sempre web listen 127.0.0.1:33211
printf 'new-password\n' | sempre web password set --stdin
sempre web password clear
sempre ui status
sempre ui install official
sempre ui install https://example.com/sempre-ui.zip --sha256 <digest>
sempre ui install ./sempre-ui.zip --sha256 <digest>
sempre ui update
sempre ui remove
```

An empty administrator password is accepted only by a same-origin UI and is
shown as a warning. A password is required for cross-origin UI access; stored
passwords use Argon2id and successful logins receive an expiring bearer
session. Changing the listener is a live rebind: Sempre opens the new socket
before closing the old one and rolls the configuration back on failure.

The official React console covers status and live traffic, proxy selection and
latency checks, providers, connections, rules, local traffic aggregation,
logs, core versions, subscriptions, validated configuration editing, listener
and password settings, and UI lifecycle management. Runtime features are also
available under `sempre runtime`; run `sempre help` for the command map.

UI archives are independent third-party components. A compatible ZIP has
`index.html` and `sempre-ui.json` at its root, declares Sempre API major 1, and
is size/path/symlink checked before an atomic activation. Only one UI is active
at a time. A locally installed custom UI is preserved by `sempre install`;
official UI installations are refreshed from the bundle or matching release.

## Services

```text
sempre service install
sempre service uninstall
sempre service start
sempre service stop
sempre service restart
sempre service status
```

`service install` registers Sempre with the native system service manager,
enables it, and starts it. It also copies Sempre and bundled resources to a
protected system executable directory, so the original download can be moved
or deleted afterwards. Core and configuration state is merged with an existing
installation. `service uninstall` removes only the service registration;
top-level `uninstall` removes the application while retaining configuration,
subscription, listener, and password unless `--purge` is supplied.

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
port supplied by the user configuration, TUN interface name, or the presence
of another proxy product. For supported cores, Sempre generates a protected
temporary runtime configuration with a random loopback control port and secret;
the original configuration is never rewritten. `status`
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
endpoint.json
resources/
|-- sempre-ui.zip
`-- SHA256SUMS
.sempre/
|-- state.json
|-- web.json
|-- cores/
|   `-- sing-box/<version>/
|-- configs/
|   `-- sing-box/<sha256>.json
|-- ui/
|   `-- current/
|-- logs/
`-- run/
```

The system service always runs the protected system executable with
`--system daemon`, even when installation was initiated from portable mode.

## Build

Go 1.25 or newer and Bun 1.3.14 are required. The backend build is pure Go and
uses `CGO_ENABLED=0`.

```text
bun install --cwd ui --frozen-lockfile
bun run lint
bun run tsc
bun run test
go test ./...
go test -race ./...
go vet ./...
go run golang.org/x/vuln/cmd/govulncheck@v1.6.0 ./...
bun run build
```

The build command validates both projects and emits Windows, Linux, and macOS
binaries for amd64 and arm64, `sempre-ui.zip`, six self-contained bundle ZIPs,
and `dist/SHA256SUMS`. Windows resources use an `asInvoker` manifest; UAC is
requested at runtime only for privileged commands.

Tagged release builds use Go 1.25.12, derive their embedded build date from the
Git commit timestamp, publish per-target CycloneDX SBOMs, and attach GitHub
artifact attestations. Release binaries are currently unsigned at the operating
system level.

Sempre itself does not redistribute sing-box. Core releases are downloaded at
runtime from the official GitHub release and verified against the SHA-256
digest supplied by GitHub's release API.

## License

Sempre is licensed under the [BSD 3-Clause License](LICENSE). Downloaded proxy
cores remain subject to their own licenses.
