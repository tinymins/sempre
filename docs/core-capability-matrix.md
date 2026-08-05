# Core Capability Model

Sempre models settings with stable product semantics and maps them to native
core fields. It does not calculate a UI from coincidentally similar JSON or
YAML paths.

## Runtime Core Status

| Core | Class | Status in Sempre | Configuration authority |
| --- | --- | --- | --- |
| sing-box | Complete cross-platform core | Stable, registered | [Configuration](https://sing-box.sagernet.org/configuration/), [TUN](https://sing-box.sagernet.org/configuration/inbound/tun/), [Clash API](https://sing-box.sagernet.org/configuration/experimental/clash-api/) |
| Mihomo | Complete cross-platform core | Stable, registered | [Configuration](https://wiki.metacubex.one/en/config/), [DNS](https://wiki.metacubex.one/en/config/dns/), [TUN](https://wiki.metacubex.one/en/config/inbound/tun/) |
| Xray-core | Complete cross-platform core | P0 design target, not registered | [Configuration](https://xtls.github.io/en/config/), [TUN](https://xtls.github.io/en/config/inbounds/tun.html), [gRPC API](https://xtls.github.io/config/api.html) |
| V2Ray-core | Complete cross-platform core | Planned evaluation, not registered | [Configuration](https://www.v2fly.org/en_US/config/overview), [API](https://www.v2fly.org/en_US/config/api.html) |
| clash-rs | Complete Clash-family core | Experimental candidate, not registered | [Project and compatibility mode](https://github.com/Watfaq/clash-rs) |
| dae | Linux eBPF gateway core | Separate gateway evaluation, not registered | [Project](https://github.com/daeuniverse/dae) |

Only registered stable adapters participate in the no-core common-field
intersection. A future or experimental entry in this table does not make its
fields visible and cannot be selected or compiled.

## Complete Semantic Field Mapping

The editor renders a field only when the configuration context contains its
capability. With a selected core, that context comes from its adapter. With no
selected core, it is the intersection of all registered stable adapters.
Linux-only capabilities are removed on other platforms.

| Sempre capability | sing-box mapping | Mihomo mapping | Display scope |
| --- | --- | --- | --- |
| `logging.level` | `log.level` / `log.disabled` | `log-level` | Both |
| `dns.local_upstream` | direct `dns.servers` entry | `direct-nameserver`, `nameserver-policy` | Both |
| `dns.remote_upstream` | detoured `dns.servers` entry and `dns.final` | selector-qualified `nameserver` | Both |
| `dns.bootstrap_upstream` | direct bootstrap DNS server | `default-nameserver`, `proxy-server-nameserver` | Both |
| `dns.bootstrap_port` | bootstrap `server_port` | Not consumed; bootstrap stays an IP resolver | sing-box only |
| `dns.bootstrap_server_name` | bootstrap `tls.server_name` | Not consumed | sing-box only |
| `dns.remote_server_name` | remote `tls.server_name` | Not consumed | sing-box only |
| `dns.remote_detour` | remote DNS server `detour` | DNS URL `#proxy` parameter | Both |
| `dns.fake_ip` | FakeIP DNS server/ranges | `enhanced-mode`, `fake-ip-range`, `fake-ip-range6`, `fake-ip-ttl` | Both |
| `dns.split` | CN DNS rules routed to local server | `nameserver-policy` with `geosite:cn` | Both |
| `dns.prefer_ipv4` | `dns.strategy=prefer_ipv4` | Not consumed | sing-box only |
| `dns.reject_https` | DNS rule rejecting query type `HTTPS` | DNS URL `disable-qtype-65` | Both |
| `dns.native` | selected complete native `dns` object | selected complete native `dns` map | Both, current target only |
| `routing.rules` | `route.rules` | `rules` | Both |
| `routing.rule_providers` | downloaded inline `route.rule_set` | `rule-providers` | Both |
| `routing.selector` | `selector` outbound | `select` proxy group | Both |
| `routing.url_test` | `urltest` outbound | `url-test` proxy group | Both |
| `transparent.tun` | `tun` inbound with `auto_route`, `auto_redirect`, `strict_route` | top-level `tun` with `auto-route`, `auto-redirect`, `strict-route` | Both |
| `transparent.tun.address` | TUN `address` | No Sempre-managed address field | sing-box only |
| `transparent.interface_policy` | TUN `include_interface` / `exclude_interface` | TUN `include-interface` / `exclude-interface` | Linux, both |
| `transparent.tproxy` | TProxy and DNS inbounds plus Sempre Linux data plane | `tproxy-port`, DNS TProxy listener/outbound plus Sempre Linux data plane | Linux, both |
| `management.connections` | Clash REST `/connections` | Clash REST `/connections` | Both |
| `management.selector_switch` | Clash REST `/proxies` | Clash REST `/proxies` | Both |
| `management.delay` | Clash REST delay endpoint | Clash REST delay endpoint | Both |
| `management.traffic` | Clash WebSocket traffic endpoint | Clash WebSocket traffic endpoint | Both |
| `management.external_api` | Sempre-authenticated reverse proxy to private Clash API | Sempre-authenticated reverse proxy to private controller | Both |
| `private_access` | managed endpoint/outbound and route/DNS rules | Not implemented | sing-box 1.12+ only |
| `native_override` | `core_overrides.sing-box` deep merge | `core_overrides.mihomo` deep merge | Selected core only |

The native field names above are checked against the official
[sing-box DNS](https://sing-box.sagernet.org/configuration/dns/),
[sing-box TUN](https://sing-box.sagernet.org/configuration/inbound/tun/),
[sing-box selector](https://sing-box.sagernet.org/configuration/outbound/selector/),
[Mihomo DNS](https://wiki.metacubex.one/en/config/dns/),
[Mihomo TUN](https://wiki.metacubex.one/en/config/inbound/tun/),
[Mihomo rule-provider](https://wiki.metacubex.one/en/config/rule-providers/),
and [Mihomo controller](https://wiki.metacubex.one/en/config/general/)
documentation. Native overrides cannot write fields owned by the structured
transparent or management settings; compilation rejects those conflicts rather
than allowing the runtime to silently replace them.

The control contract carries an explicit protocol. Today both registered cores
use `clash-rest`; Xray and V2Ray would use `grpc`. The Sempre UI can normalize
connections, selectors, delay, and traffic above those transports without
claiming that a gRPC core natively implements Clash API.

## Configuration Ownership

| Setting | Canonical profile location | Not stored in |
| --- | --- | --- |
| Main TProxy listener | `transparent_proxy.tproxy.listen_port` | DNS |
| Transparent DNS listener | `transparent_proxy.tproxy.dns_listen_port` | DNS upstreams |
| Local/bootstrap/remote resolvers | `dns.shared` | Runtime listener settings |
| External controller, secret, UI, CORS | `management_api` | DNS or native core override |
| Native top-level document | `core_overrides.<core-id>` | One hardcoded field per core |

Catalog schema 5 migrates the old DNS listener/API keys and typed sing-box
override into these owners. Deprecated keys are removed on the next write.
Unknown `core_overrides` keys are retained but ignored until an adapter with
that exact Core ID is registered.

## Protocol Compatibility

Each adapter emits a separate
`protocol + transport + security + minimum_version` matrix. This table records
the complete Sempre parser/compiler path, not every protocol a core can run
natively.

| Protocol | sing-box | Mihomo | Transport/security represented by Sempre |
| --- | --- | --- | --- |
| HTTP | Yes | Yes | TCP; plain or TLS |
| SOCKS5 | Yes | Yes | TCP/UDP; plain |
| VMess | Yes | Yes | TCP, WebSocket, HTTP, gRPC; plain/TLS |
| VLESS | Yes | Yes | TCP, WebSocket, HTTP, gRPC; plain/TLS/REALITY |
| Shadowsocks | Yes | Yes | TCP/UDP; cipher |
| ShadowTLS | Yes, via converted SS plugin | Yes, via SS plugin | TCP/TLS |
| Trojan | Yes | Yes | TCP, WebSocket, gRPC; TLS/REALITY |
| Hysteria | Yes | Yes | UDP/TLS |
| Hysteria 2 | Yes | Yes | UDP/TLS |
| TUIC | Yes | Yes | UDP/TLS |
| AnyTLS | 1.12.0+ | Yes | TCP/TLS; Mihomo does not claim REALITY |

NaiveProxy is native in sing-box but is not yet declared because Sempre does
not currently parse and compile Naive nodes end to end. A protocol being in
this matrix does not register its standalone program as a runtime core. A
complete core adapter must separately implement installation, version
detection, compilation, official validation, start, stop, control, and
diagnostics before it joins the stable intersection.
