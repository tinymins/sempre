# Linux Transparent Gateway

Sempre supports transparent routing with the stable `sing-box` and `mihomo`
adapters on Debian, Ubuntu, and Proxmox VE. The profile stores one
core-independent capture intent; each core adapter generates its inbound while
the Linux backend owns nftables, marks, and policy routes.

## Modes

| Mode | Data plane | Host traffic | LAN forwarding |
| --- | --- | --- | --- |
| `tun-router` | Core TUN with automatic routes and redirect | Always | Yes |
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

For sing-box the generated inbound uses `interface_name: sempre-tun`,
`auto_route: true`, `auto_redirect: true`, `strict_route: true`, and
`stack: system`. Sempre resolves the final `/30` address and exclusions in the
private runtime copy. For Mihomo it generates the equivalent `tun` keys using
their kebab-case names. Include/exclude interface policies are mapped to each
core's native fields. Sempre validates the final runtime copy with the selected
core binary.

sing-box owns the TUN interface and its `inet sing-box` nftables table. Sempre
does not rewrite the host default route, Docker rules, LXC rules, PVE firewall
rules, or user nftables tables.

## TProxy

Sempre owns only these names and identifiers:

| Resource | Value |
| --- | --- |
| nftables table | `sempre_tproxy` in IPv4 and IPv6 families |
| packet mark | `0x53500001` |
| core bypass mark | `0x53500002` |
| policy route table | `20240` |
| policy rule priority | `20240` |
| policy object protocol | `253` |

TCP and UDP from selected LAN interfaces are captured in prerouting. TCP and
UDP port 53 are sent to the configured DNS inbound. With `capture_host` enabled,
OUTPUT packets are marked and routed back through prerouting. The selected core
uses the separate bypass mark to prevent its own outbound sockets from looping.
DNS has a distinct listener port because port 53 packets are captured separately,
but both ports belong exclusively to `transparent_proxy.tproxy`; they are not DNS
upstream settings.

Apply is transactional: policy routes are installed before nftables begins
capturing traffic, and any failure removes all Sempre-owned state. Stop,
restart, mode changes, core exits, and idle startup all run the same idempotent
cleanup. Sempre marks its table and policy objects, rejects ownership
collisions, and never flushes a ruleset or deletes another table.

## DNS Routing

The managed DNS model has local, bootstrap, and remote upstreams. Domestic
domains use local DNS, proxy node names use the direct bootstrap resolver, and
remote DNS follows the configured foreign selector. sing-box emits
`dns.final: remote`; Mihomo emits `respect-rules`, `nameserver-policy`, and a
selector-qualified remote nameserver. The split remains active when FakeIP is
disabled. `prefer_ipv4` is shown only for cores that implement it.

## Management API And MetaCubeX

The current stable cores expose a private Clash-compatible controller on a
random loopback port with a random internal secret. `management_api` optionally
starts a Sempre-authenticated reverse proxy at the user controller address.
HTTP and WebSocket endpoints, including connections, proxies, providers, rules,
traffic, and delay, are forwarded. The proxy serves `external_ui` at `/ui/` and
applies the configured origin and private-network policy. MetaCubeX connects to
this external Sempre endpoint; the running core remains sing-box or Mihomo.

Binding to `0.0.0.0` exposes control over proxy selection and connection data.
Use a strong Secret and enforce host firewall restrictions. The external
listener is disabled by default.

## Advanced Overrides

The profile stores advanced documents in the open
`core_overrides.<core-id>` map. The editor displays only the selected target's
document and preserves every other key, including unknown future core IDs.
Known overrides are deep-merged into that core's generated configuration and
survive saves, refreshes, scheduled updates, recompilation, restarts, and
reboots.

In managed Linux modes, fields owned by the runtime plan cannot also be set in
the native override. Structured management API fields likewise cannot be
duplicated in a core override. Select `disabled` when a fully custom inbound
topology is required.

## Diagnostics And Recovery

Run:

```sh
sempre doctor
```

Required checks cover the prepared runtime hash, profile build revision, TUN
interface, core auto-redirect rules, Sempre TProxy rules, policy routing,
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
