# Relay egress policy

Only administrators and superadministrators can read or change the global relay
policy. It applies to upstream requests made by the public resource relay;
subscription-source proxy choices remain independent. The server remains
core-free and resolves managed connections through the optional `sempre-proxy`
service.

The existing proxy-provider page is named **Proxy settings**. Provider IDs and
subscription-source selections retain their existing meaning. The administrator
settings page adds **Relay egress**, with a default outbound, optional exact-host
overrides, explicit save, and a saved-policy URL probe. Selecting an overseas
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

OpenWrt exports wrap custom binary providers and the built-in CN/HK GeoIP and CN
GeoSite resource URLs with the raw endpoint. Their `binary` format is retained,
including when inferred from the original `.srs` extension. Other rule providers
retain their existing conversion endpoints. Wrapped client downloads use the
direct outbound to reach Sempre; upstream egress is selected by the server.

Public relay destinations must resolve to public addresses. Direct connections
pin the validated DNS results. Managed or environment proxies can perform their
own destination DNS resolution, so local validation does not pin their remote
resolution. Recursive requests to this service's relay paths are rejected.

Raw responses have no server-side body cache. Rule conversion retains its
existing retry/cache behavior, with saved-policy and referenced-provider
revisions included in cache identity. Administrator probes bypass body caches.

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
Malformed URLs return HTTP 400. Tests use the saved policy; unsaved form changes
have no effect.

Migration `0012_relay_egress.sql` initializes the system fallback and reference
protection. Run the normal `sempre-server migrate` step before starting this
server version.
