# CN system routes before TUN

Enable **CN direct system routes** in the network settings and restart the core.
The setting is off by default, requires system mode, local network mode,
sing-box, and the managed DNS frontend. It forces real-IP DNS and an enabled
`geoip-cn` destination-only rule set. Conditional, inverted, empty, or unsafe
rule sets fail activation before the TUN fallback is enabled.

CN destinations use explicit routes through the original interface and gateway.
Other destinations use two `/1` TUN routes per active address family. Existing
connected and more-specific system routes retain their precedence. Explicit
private-access capture routes are preserved and must not overlap CN prefixes.
The runtime removes native automatic TUN routing and the previous whole-DNS-IP
exclusions, so an ordinary application connection to `1.1.1.1` can enter TUN.
Core and frontend DNS upstream sockets use the physical interface to prevent
recapture. Linux additionally uses the existing bypass socket mark.

| Platform | Installation | Ownership |
| --- | --- | --- |
| macOS | Batched Darwin routing socket messages | `RTF_PROTO1` and full route identity |
| Linux | `ip -batch`, table 20241, rules 20241–20243 | Protocol 253 and full route identity |
| Windows | IP Helper through batched PowerShell/C# calls | Metric 53717, protocol 3 and full route identity |

The route count is the normalized CN CIDR count for active address families,
plus two fallback routes per family and any private capture routes. No complement
of the CN set is expanded. IPv4 and IPv6 currently require the same physical
interface because the managed core has one default egress binding.

## Lifecycle

1. Resolve and validate a cached CN snapshot before starting capture.
2. Prepare TUN and promote the healthy real-IP DNS frontend.
3. Flush the system DNS cache on macOS/Windows, install CN routes, then TUN fallback.
4. Validate representative CN destinations and `1.1.1.1` with native route lookup.
5. Poll the rule snapshot and uplinks every three seconds. Apply gateway and CIDR
   differences; restart the core when the bound physical interface changes.
6. Withdraw fallback before deleting CN routes on stop or error. Persist a
   write-ahead journal at `data/cn-routes.json`; recover it on the next daemon start.

Managed remote rule updates first install the union of old and new CN prefixes,
then publish the new snapshot. The monitor subsequently removes obsolete routes.
Failed cleanup retains the journal for retry. Caller cancellation cannot race
an in-flight native batch against cleanup.

## Acceptance and limits

This is a routing guarantee while the managed routes are installed. A pure
CN-route-plus-default design cannot provide an absolute zero-packet guarantee
when another actor or the kernel suddenly removes physical-interface routes
before Sempre can withdraw TUN fallback. Abrupt daemon termination also relies
on the next service start to recover surviving routes. A stricter guarantee
requires an additional kernel enforcement mechanism or complement routes.

Applications retaining old FakeIP answers must resolve again; flushing system
DNS does not invalidate every application's private cache. Explicit application
interface binding, other VPNs, and earlier policy routes can override ordinary
route selection. Activation fails when the acceptance route lookups disagree.

Before production acceptance, verify on each supported OS with the real core:

| Check | Expected result |
| --- | --- |
| CN literal and real-IP domain connections | Physical interface; absent from TUN packet capture |
| Ordinary literal `1.1.1.1` connection | Managed TUN, then the configured core outbound |
| Core/proxy/DNS upstream connections | Physical egress; no routing loop |
| Rule refresh and repeated network switches | Updated gateway/CIDRs; unrelated routes preserved |
| Normal stop, startup after forced termination, partial installation failure | Owned fallback removed first; journal cleared only after successful cleanup |
| Full CN snapshot | Record install/remove time and route-table resource cost |

The Linux kernel regression runs only in an isolated network namespace with
NET_ADMIN and iproute2, never the host namespace:

```sh
SEMPRE_CN_NETNS_TEST=1 cargo test --manifest-path=rust/Cargo.toml \
  -p sempre-transparent --test cn_routes_linux -- --ignored --nocapture
```

With `SEMPRE_CN_SNAPSHOT` pointing to the exported source-format CN file,
the same check exercises the complete snapshot. On 2026-10-12, the supplied
snapshot normalized to 5,790 IPv4 and 2,255 IPv6 CIDRs. In the local Docker Linux
namespace with dummy interfaces, initial installation took 284 ms and recovery
cleanup after updates took 87 ms. These measurements cover kernel route
operations; they do not establish packet throughput or macOS/Windows costs.

The macOS read-only routing-socket check sends only `RTM_GET`:

```sh
cargo test --manifest-path=rust/Cargo.toml -p sempre-transparent \
  native_get_loopback_route_is_read_only -- --ignored --nocapture
```
