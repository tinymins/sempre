# Linux Transparent Gateway

Sempre supports transparent sing-box routing on Debian, Ubuntu, and Proxmox
VE. The feature is available only for the sing-box core; Mihomo behavior is
unchanged.

## Modes

| Mode | Data plane | Host traffic | LAN forwarding |
| --- | --- | --- | --- |
| `tun-router` | sing-box TUN with `auto_route` and `auto_redirect` | Always | Yes |
| `tproxy` | Sempre-owned nftables table, fwmark, policy rule, and local route table | Optional | Selected LAN interfaces |
| `disabled` | No Linux transparent routing managed by Sempre | No | No |

New and migrated profiles default to `tun-router`. Sempre chooses an unused
IPv4 `/30` when `tun.address` is empty. Existing local, container, and VPN
prefixes are added to `route_exclude_address` according to the profile flags.
An explicit address that overlaps an interface or route is rejected before
the core starts.

For a typical Proxmox VE router with `vmbr0` as WAN/management and `vmbr1` as
LAN, the UI recommends `vmbr1` for TProxy capture. The default-route bridge is
never recommended as a LAN interface.

## Prerequisites

- Run the protected Sempre system service as root.
- Enable IPv4 forwarding before routing LAN clients. Sempre checks
  `net.ipv4.ip_forward` but never changes this global setting.
- Keep the management and LAN prefixes assigned to real interfaces so Sempre
  can exclude them from transparent routing.
- Keep a direct console or other recovery path when first enabling gateway
  routing on a remote host.

The service fails before committing a deployment if the TUN address conflicts,
a configured LAN interface is missing, forwarding is disabled, a TProxy policy
table collides with user state, or the generated runtime configuration is
invalid. The previous deployment is restored when one exists.

## TUN Router

The generated inbound uses `interface_name: sing-box`, `auto_route: true`,
`auto_redirect: true`, `strict_route: true`, and `stack: system`. Sempre resolves
the final TUN address and route exclusions in the private runtime copy, then
validates that exact copy with the installed sing-box binary.

sing-box owns the TUN interface and its `inet sing-box` nftables table. Sempre
does not rewrite the host default route, Docker rules, LXC rules, PVE firewall
rules, or user nftables tables.

## TProxy

Sempre owns only these names and identifiers:

| Resource | Value |
| --- | --- |
| nftables table | `sempre_tproxy` in IPv4 and IPv6 families |
| packet mark | `0x53500001` |
| sing-box bypass mark | `0x53500002` |
| policy route table | `20240` |
| policy rule priority | `20240` |
| policy object protocol | `253` |

TCP and UDP from selected LAN interfaces are captured in prerouting. TCP and
UDP port 53 are sent to the configured DNS inbound. With `capture_host` enabled,
OUTPUT packets are marked and routed back through prerouting; sing-box outbound
sockets use the separate bypass mark to prevent loops.

Apply is transactional: policy routes are installed before nftables begins
capturing traffic, and any failure removes all Sempre-owned state. Stop,
restart, mode changes, core exits, and idle startup all run the same idempotent
cleanup. Sempre marks its table and policy objects, rejects ownership
collisions, and never flushes a ruleset or deletes another table.

## DNS Routing

The default Linux sing-box configuration uses local DNS for `geosite-cn`, a
direct bootstrap DoT server for proxy node hostnames, and a remote DoT server
detoured through the configured foreign selector. `dns.final` is `remote`, so
the same split remains active when FakeIP is disabled. `prefer_ipv4` is enabled
by default and can be changed in the DNS editor.

## MetaCubeX

The core controller always remains on a random loopback port with a random
internal Secret. When the external Clash API is enabled, Sempre listens on the
configured `external_controller`, authenticates the fixed user Secret, and
proxies HTTP and WebSocket requests to the internal controller. The proxy also
serves `external_ui` at `/ui/` and applies the configured origin and private
network access policy.

Binding to `0.0.0.0` exposes control over proxy selection and connection data.
Use a strong Secret and enforce host firewall restrictions. The external
listener is disabled by default.

## Advanced Overrides

The profile's Advanced Config editor stores a structured top-level JSONC
object. It is deep-merged into the generated configuration and survives saves,
subscription refreshes, scheduled updates, recompilation, service restarts,
and host reboots.

In managed Linux modes, top-level `inbounds` cannot be overridden and TUN
`route.auto_detect_interface` cannot be disabled. Structured external Clash
API fields also cannot be duplicated under `experimental.clash_api`. Select
`disabled` when a fully custom inbound topology is required.

## Diagnostics And Recovery

Run:

```sh
sempre doctor
```

Required checks cover the prepared runtime hash, profile build revision, TUN
interface, sing-box auto-redirect rules, Sempre TProxy rules, policy routing,
listeners, LAN interfaces, IPv4 forwarding, split DNS, and domestic/foreign
route structure. TProxy capture counters report whether host, LAN, and DNS
packets have actually reached the data plane since startup. Missing traffic and
external DNS or HTTP probe failures are warnings because they may simply mean
the relevant traffic source is idle or the Internet is unavailable.

To recover, stop the service from the host console. Normal stop removes the
external controller and all Sempre-owned TProxy state. If the daemon was killed
without cleanup, the next root-owned start removes stale Sempre state before
launching the core. Sempre does not automatically change SSH routes, the host
default route, or `ip_forward`.
