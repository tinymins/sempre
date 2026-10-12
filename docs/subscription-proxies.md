# Subscription proxies

The server UI's **订阅代理 / Subscription proxies** page manages application-level
proxies used to download subscription sources. Each URL source selects **系统模式 /
System mode** or one named proxy from a flat list. System mode retains the server's
default HTTP-client networking behavior. A selected proxy never silently falls
back to system mode. Remote rule sets use system mode; the old two-route race is
removed. Generated client routing, DNS, and node configuration are unaffected.

## Deployment

`ohmywrt/sempre` remains core-free. The optional `ohmywrt/sempre-proxy` image runs
sing-box 1.14.3 and Xray 26.3.27. Both Linux amd64 and arm64 engine images are
pinned by their multi-platform SHA-256 digests. Release tags publish both images with the same version/channel.
The proxy service needs no TUN, host networking, Docker socket, or added capabilities.
Do not publish its control port or generated proxy listener ports to the Internet.

Set these alongside the existing Compose variables, using two independent secrets:

```dotenv
SEMPRE_PROXY_URL=http://sempre-proxy:8790/
SEMPRE_PROXY_TOKEN=<at least 32 random characters>
SEMPRE_PROXY_KEY=<base64 encoding of 32 random bytes>
```

Generate a token with `openssl rand -hex 32` and an encryption key with
`openssl rand -base64 32`. Keep the encryption key with database backups; losing it
makes saved proxy configurations unreadable. Both services need the same token;
only the main server needs the encryption key.

Initialize the schema using the existing `sempre-server migrate` command, then run:

```sh
docker compose -f rust/compose.yml --profile subscription-proxy up -d --build
```

The optional `SEMPRE_PROXY_PROBE_URL` runtime setting defaults to
`https://www.baidu.com/`. It must be reachable through the configured upstream.
Passing the startup probe does not guarantee access to a particular subscription;
use the source debugger to verify the real destination.

Migration `0011_subscription_proxies.sql` creates the new table only. It does not
convert `fetchMode`, import `DIRECT_PROXY_URL`, or modify existing sources. Before
switching an existing deployment, manually remove old `fetchMode` fields and set
each source's `proxyId` to null (system mode) or the intended proxy UUID. The new
server rejects unknown source fields rather than interpreting old modes.

## Connections and lifecycle

Add one URI, a Clash document containing one node, or a single native outbound.
The manual form covers HTTP, SOCKS5, Shadowsocks, VMess, VLESS, Trojan, Hysteria2,
and TUIC. Native sing-box outbounds additionally allow AnyTLS. Native configuration
stays on its original engine; URI/node conversion chooses a lossless supported
engine or reports unsupported fields. Engine configuration validation and a real
HTTP request run before the candidate becomes active. Xray 26.3.27 rejects
`allowInsecure: true`; use a valid certificate, native certificate pinning, or
choose sing-box for a node that explicitly disables certificate verification.

Full core configurations, chained proxies, external plugins, filesystem-backed
credentials, and host network controls are rejected. Native advanced fields are
validated by the pinned engine. Each enabled proxy has its own process and
authenticated HTTP listener on the private container network. Limits are eight
saved proxies per user and 64 active proxies per runtime.

Saving an enabled connection starts it asynchronously. Configuration updates keep
the previous working instance until the candidate passes its checks; old requests
can drain for two minutes. Desired and last working configurations are encrypted
separately in PostgreSQL. After runtime restart, reconciliation restores enabled
connections and the last working version of a failed edit. Runtime/control errors
are visible in the list; ordinary core output is discarded to avoid credential
disclosure. The server reconciles saved state every 30 seconds.

Only the owner can edit configurations and credentials. Authorized users can use
and test shared proxies. Public subscription generation resolves the subscription
owner's proxy permission. Proxy IDs and active revisions partition source caches;
public artifact revisions include proxy references and active revisions. Changing
a proxy does not change the source's stable identity. Referenced proxies cannot be
deleted or have a still-referencing owner's access revoked.

## API

All endpoints require an authenticated server session:

| Method | Path | Purpose |
| --- | --- | --- |
| GET | `/api/v1/subscription-proxies` | Redacted list and runtime configuration availability |
| POST | `/api/v1/subscription-proxies` | Create an encrypted connection |
| GET | `/api/v1/subscription-proxies/{id}` | Owner-only edit configuration |
| PATCH | `/api/v1/subscription-proxies/{id}` | Update name, connection, enabled state, or grants |
| DELETE | `/api/v1/subscription-proxies/{id}` | Delete an unreferenced connection |
| POST | `/api/v1/subscription-proxies/{id}/test` | Check the active connection |

Creation accepts `name`, `content`, `engine` (`null`, `sing-box`, or `xray`),
`enabled`, and `authorizedUserIds`. On update, omitting `content` preserves the
connection. A URL source stores `proxyId`; null/absence selects system mode.
