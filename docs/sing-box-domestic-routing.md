# sing-box domestic routing and DNS ownership

The managed DNS renderer owns both its resolver tags and its ordered domestic
route stages. The general route assembler only inserts those stages; it must not
inspect serialized rule-set tags to invent DNS actions.

## Routing contract

Existing private-access routes, private-IP rules, priority providers, and explicit
user rules keep their existing precedence. The managed domestic stages follow:

1. `geosite-cn -> direct`: known domestic domains do not require a remote lookup.
2. `resolve(server=remote)`: resolve remaining domain destinations to real IPs.
3. `geoip-cn -> direct`: classify those real destination addresses.
4. Non-priority providers, then the configured final outbound.

FakeIP restores the original domain, not the real addresses needed by GeoIP.
The same resolve stage also handles unresolved domain requests received by SOCKS.
It does not replace an already known literal destination address.

An explicit resolve server bypasses DNS routing and can change the eventual
connection target, not just the classification result. Therefore the unknown
domain stage must never use the local resolver. The generated remote resolver
retains the user's remote DNS address and outbound detour.

Managed modern DNS also enables `independent_cache`: older supported cores can
otherwise satisfy `resolve(remote)` with an answer previously cached by `local`.
Resolver selection and cache isolation are both necessary.

## Boundaries

- Disabling the CN IP rule set removes both the resolve and GeoIP stages.
- Disabling the CN domain rule set removes only the Geosite stage.
- Legacy v11 does not receive the new resolve action.
- Native DNS overrides own their resolver tags; no generated `remote` resolve
  action is inserted into that mode. User-supplied route overrides remain final.
- Bootstrap DNS, private DNS, the managed frontend, and explicit DNS settings
  retain their existing ownership. This change does not migrate saved profiles.
- Unknown domains can still receive a foreign CDN address from remote DNS. That
  is preferable to trusting a poisoned local answer. Known domestic domains
  bypass this lookup; user/provider routing precedence is not reordered.
- Build schema 5 invalidates previously compiled schemas, including hotfix 4.

## Verification

`tests/dns.rs` checks ordering, resolver/detour selection, cache isolation,
independent switches, native overrides, version support, and desktop modes.

The offline real-core regression runs without TUN, system DNS changes, real
proxy credentials, or public DNS. It compiles a managed configuration, substitutes
loopback DNS/proxy transports, and verifies actual outbound destinations:

```sh
cargo build --manifest-path=rust/Cargo.toml -p sempre-converter-cli
bun rust/scripts/dns_route_smoke.ts /path/to/sing-box-1.12 /path/to/sing-box-1.13 /path/to/sing-box-1.14
```

It covers foreign domains, unknown domestic IPs, known domestic domains, and a
poisoned local answer cached before remote resolution, in FakeIP and real-IP
modes. Public-network acceptance additionally requires requests through the real
managed instance, checking TLS, the resolved address, and the matched outbound.

See the upstream [resolve action contract](https://sing-box.sagernet.org/configuration/route/rule_action/#resolve).
