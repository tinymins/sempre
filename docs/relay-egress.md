# Relay egress policy

Only administrators and superadministrators can read or change the global relay
policy. It applies to business resource requests, including public resource relay
requests, rule conversion, rule-provider snapshots prepared for subscription
exports and debug output, and network GeoIP/Geosite lists, including the APNIC
fallback for GeoIP. Subscription-source proxy choices take precedence:
a configured source proxy is used explicitly, and null/absent `proxyId` means
direct access without environment proxies. Node tests and startup probes use the
specific proxy being tested. The server remains
core-free and resolves managed connections through the optional `sempre-proxy`
service.

The existing proxy-provider page is named **Proxy settings**. Provider IDs and
subscription-source selections retain their existing meaning.
**Proxy settings → Proxy rules** provides a default outbound, optional exact-host
overrides in a compact table, and a saved-policy URL probe. Selecting an overseas
provider as the default is sufficient when no host overrides are needed.

## Public resource endpoints

| Path | Response |
| --- | --- |
| `/api/proxy/fetch?url=<encoded-upstream-url>` | Original upstream bytes, including binary `.srs` resources |
| `/api/proxy/sing-box/convert/rule?url=...` and version variants | Converted rule-set JSON |

Both paths use the same saved relay egress policy. The raw resource endpoint
returns the final upstream status and relevant content/cache headers, follows
redirects server-side, and does not redirect clients back to the upstream URL.
It does not forward client cookies or authorization. Downloads are limited to
32 MiB, five redirects, and bounded DNS/connect/request timeouts.

Server exports wrap remote rule resources after compilation, including custom
configuration overrides and compiler-generated providers. Clash, Clash Meta,
and Clash RS rule-provider URLs and explicit geodata URLs use the raw endpoint,
retaining their original response format. Sing-box rules generated from Clash
providers use the versioned conversion endpoint; native source JSON and binary
rule resources use the raw endpoint. Binary formats inferred from `.srs` are
retained. Already wrapped URLs remain unchanged. This applies to direct exports,
manifest artifacts, and debug output.
Wrapped sing-box downloads use the direct outbound to reach Sempre; upstream
egress is selected by the server.

URL wrapping is owned solely by `sempre-server::resource_export`. Subscription
preparation retains original resource URLs; the unified export entry resolves
source-format metadata, compiles the configuration, then rewrites remote rule
URLs once and calculates the final artifact hash. Direct downloads, manifest
artifacts, and debug output all use that entry. There is no separate OpenWrt
URL-rewriting step, and the pure converter receives no relay URL configuration.

Resource requests use the selected outbound's DNS and connection behavior without
a local public-IP precheck or DNS pinning. Recursive requests to this service's
relay paths are rejected.

Raw responses have no server-side body cache. Rule conversion and rule-provider
snapshots retain their existing retry/cache behavior. Their cache identities, the
24-hour in-memory network GeoIP/Geosite cache, and the revision identity used for
final subscription artifact fallback all include the saved-policy and referenced
proxy revisions. Changes to those revisions prevent reuse of a cached result or
fallback artifact from the previous policy. Administrator probes bypass body caches.

## Administrator API

| Method | Path | Purpose |
| --- | --- | --- |
| GET | `/api/v1/admin/relay-egress` | Read saved policy and administrator-owned proxy choices |
| PUT | `/api/v1/admin/relay-egress` | Atomically replace the policy using its current revision |
| POST | `/api/v1/admin/relay-egress/test` | Fetch a URL using the saved policy, bypassing response caches |

GET returns `revision`, `defaultOutbound`, `hostRules`, and `availableProxies`.
Each available proxy contains `id`, `name`, `enabled`, and `status`; connection
configuration and listener credentials are never returned. PUT accepts only
`revision`, `defaultOutbound`, and `hostRules`. A stale revision returns HTTP 409.

An outbound is exactly one of:

```json
{"type":"system"}
{"type":"direct"}
{"type":"proxy","proxyId":"00000000-0000-0000-0000-000000000000"}
```

System mode preserves HTTP-client environment proxy behavior and is the initial
fallback. Direct mode explicitly disables environment proxies. Managed proxy mode
requires an enabled connection owned by an administrator when saving. If that
connection later becomes disabled or unavailable, requests fail without falling
back to direct or system mode.

Each host rule contains `host` and `outbound`. Hostnames are exact matches,
normalized case-insensitively; schemes, ports, paths, and wildcards are rejected.
Rules do not match subdomains implicitly. A request with no matching rule uses
`defaultOutbound`. Redirects select the outbound again for the next destination.

Referenced connections cannot be deleted. Their owners cannot be deleted or
demoted out of administrator roles until another outbound replaces the reference.
The connection list's `referenceCount` includes both subscription references and
relay policy slots. Connection permissions granted to subscription users do not
control the global relay policy.

The test accepts `{"url":"https://example.com/resource"}`. Results contain `ok`,
`host`, `matchedHost` (null for the fallback), `outbound`, `outboundName`,
`httpStatus`, `elapsedMs`, and `bodyBytes`. Network or selected-connection failures
return a structured result with `ok:false`, `httpStatus:null`, and `error`.
Malformed URLs return HTTP 400. Tests use the saved policy and remain disabled
while changes are pending. Dropdown selections and row deletion save immediately;
host inputs save on blur only when changed. Blank new rows remain local until a
host is entered. Consecutive edits save sequentially against the latest revision.
Failed saves retain the edited values with a retry action; revision conflicts
require reloading the policy.

Migration `0012_relay_egress.sql` initializes the system fallback and reference
protection. Run the normal `sempre-server migrate` step before starting this
server version.

The control UI groups these settings under **Proxy settings → Proxy rules**.
The **Proxy nodes** tab manages outbound providers. Rule editing retains the
existing administrator permission requirement; the admin settings page no
longer contains the relay controls.

## Custom-node bindings

Proxy settings support binding an owned or shared custom node in addition to
imported configurations and manual entry. Create or edit a proxy with
`customNodeId` instead of `content`. Edit and list responses return the binding
as `customNodeId` (null for independent configurations). Supplying `content`
switches back to an independent configuration; enable/disable requests that
omit both fields retain the binding.

Saving a node configuration atomically updates its bound proxies' encrypted
configurations and desired revisions, then schedules runtime reconciliation.
The selected engine must support the new configuration or the node edit fails.
The existing last-working-version behavior applies if runtime startup fails.
A bound node cannot be deleted, and its owner cannot revoke access from a user
whose proxy still binds it. Replace or unbind those proxies first. Proxy sharing
continues to control who can use the resulting outbound independently of node
sharing.

Migration `0013_proxy_custom_node.sql` adds the binding and deletion protection.
Run `sempre-server migrate` before using this version.

Migration `0015_relay_default_hosts.sql` adds `raw.githubusercontent.com` and
`cdn.jsdelivr.net` when missing, with each new rule initialized to the current
default outbound. Existing matching rules remain unchanged. These are ordinary
host overrides: their outbounds can be edited independently and do not change
when the default outbound is edited later. The access-test field initially uses
`https://raw.githubusercontent.com/dler-io/Rules/refs/heads/main/Clash/Provider/AdBlock.yaml`.
