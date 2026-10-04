# Toolbox subscription server contract

This document records the frozen server contract. The source
baseline is OhMyWrt Toolbox `master@969c0db` and Sempre `main@e88cea6`.

## Product and data boundary

The canonical subscription record is Toolbox `proxy_subscribes`, including its
separate, immutable `url` value. `id` is the internal UUID; `url` is the stable
public subscription identifier. Saving a record is explicit and immediately
affects the next public request. It does not require a compile, publish, or
share step. The server fetches sources and renders target configurations but
never installs or starts a proxy core. TUN, DNS, and platform settings in a
generated configuration remain valid output intent.

Toolbox `users`, `sessions`, `system_settings`, `proxy_custom_nodes`,
`proxy_subscribe_custom_nodes`, and `proxy_access_logs` are also canonical.
Subscription access follows the subscription owner and `authorized_user_ids`.
The existing Sempre `profiles`, `shares`, and `artifacts` tables
are a different product model and must not receive Toolbox subscriptions.

| Table | Contract |
| --- | --- |
| `users` | Preserve UUID, name, email, Argon2 password hash, role, and settings. |
| `sessions` | Preserve UUID `id`, user ID, and expiry. Browser auth uses `SESSION_ID` HttpOnly cookie. |
| `system_settings` | Keep `allow_registration` authoritative. |
| `proxy_subscribes` | Preserve every existing `id`, `user_id`, `url`, config column, authorized user list, and timestamps. |
| `proxy_custom_nodes` | Preserve owner, JSONC `content`, and authorized user list. |
| `proxy_subscribe_custom_nodes` | Preserve assignment, `enabled`, and `position`; disabled assignment remains available to the editor. |
| `proxy_access_logs` | Preserve subscription, format, IP, user agent, node count, and access time. |

The old server applied `sqlx::migrate!("./migrations")` at startup.
That is unsafe against a Toolbox database: its first migration creates
`users` and `sessions` unconditionally, while Toolbox already has different
versions of both tables. In particular, Sempre's `sessions.token_hash BYTEA`
primary key differs from Toolbox's `sessions.id UUID` primary key. The
similarly named Sempre `custom_nodes` table is not Toolbox
`proxy_custom_nodes`.

The new server's `migrate` command explicitly initializes a fresh database
with the Toolbox business tables; ordinary startup only checks the schema.
Migration `0007` removes the Workspace tables and setting that earlier local
development migrations created. The applied `0001` and `0005` files remain
unchanged so the migration ledger stays valid. `users`, subscriptions, node
authorization, invitations, and avatars remain independent of those tables.
Existing Toolbox database migration has not been implemented or accepted; it
must preserve rows in place when undertaken. Startup never applies Sempre's old
`0001`–`0006` migrations to it. The existing local Sempre Docker and
profile test databases are outside this migration. The product server uses one
subscription model; unused profile/share/artifact modules are removed as the
replacement is completed.

## Authentication and permissions

| Operation | Allowed identity |
| --- | --- |
| Read subscription, preview, trace, debug | Owner or authorized user. |
| Edit subscription config, select assigned nodes | Owner or authorized user. |
| Edit authorized user list, delete subscription | Owner only. |
| Manage global node content | Node owner or authorized user; only owner changes its authorization or assignments. |
| Read generated public URL | Anyone holding the opaque `url`; no session. |

Assigning a reusable node to a subscription grants that subscription the right
to render the node. It does not grant its owner or authorized editors global
permission to edit the node. Editing the subscription authorization list
requires `confirmShareAssignedNodes` when assigned nodes would become visible
to new users; the confirmation does not expand the node's global ACL. The
node list hides assignments to subscriptions a viewer does not own.

The server should filter visible subscriptions in SQL by owner or membership
in `authorized_user_ids`. Toolbox currently loads non-owned records into the
application and filters there, which should not be copied. Writes and node
assignment changes must share a transaction. A failed build cannot roll back
an already accepted save; syntax validation of edited fields may still reject
invalid input before persistence.

Reusable node content is one JSONC proxy object with nonempty `name`, `type`,
and `server`, plus a nonzero port. Node lists use creation time descending.
Only the node owner may change its authorization or subscription assignments;
an authorized editor may change its content. An assignment already present may
be retained even if the node owner later loses edit access to that subscription.
Adding an assignment requires current subscription edit access. Removing an
enabled assignment requires `confirmUnassignEnabled`; a new assignment starts
disabled and is enabled or ordered through subscription selection.

## Management API

The server UI uses JSON with camelCase keys. All routes below except
the public routes require the Toolbox session cookie. `PATCH` changes only
present fields; explicit `null` clears a nullable field. No request takes a
client-chosen public `url`.

| Method | Route | Purpose |
| --- | --- | --- |
| `POST` | `/api/v1/auth/register` | Register when `system_settings.allow_registration` permits it. |
| `POST` | `/api/v1/auth/login` | Create a seven-day Toolbox UUID session and set `SESSION_ID`. |
| `POST` | `/api/v1/auth/logout` | Revoke session and clear cookie. |
| `GET` | `/api/v1/auth/me` | Return user `id`, `name`, `email`, `role`, `settings`. |
| `GET` | `/api/v1/users` | Candidate users for subscription and node authorization. |
| `GET, POST` | `/api/v1/subscriptions` | List visible subscriptions or explicitly save a new one. |
| `GET, PATCH, DELETE` | `/api/v1/subscriptions/{id}` | Read, explicitly save changes, or owner-delete. |
| `GET` | `/api/v1/subscription-defaults` | Return current server `ruleList`, `group`, `filter`, `customConfig`, `dnsConfig` defaults as editor text. |
| `GET` | `/api/v1/subscriptions/{id}/stats` | Access counts, node count, and recent accesses. |
| `POST` | `/api/v1/subscriptions/{id}/preview-nodes` | Preview effective saved config for a target. |
| `POST` | `/api/v1/subscriptions/{id}/trace-node` | Trace a saved node and target. |
| `POST` | `/api/v1/subscriptions/debug` | Stream compilation of an unsaved complete draft for a target; no persistence. |
| `POST` | `/api/v1/subscriptions/{id}/debug` | Stream compilation of the saved subscription for `{target}` with its saved node assignments and source identity. |
| `POST` | `/api/v1/subscriptions/debug-source` | Stream source inspection; production mode requires saved `subscriptionId` and zero-based `sourceIndex`. |
| `POST` | `/api/v1/subscriptions/clear-cache` | Explicitly clear one editable subscription's saved source snapshots. |
| `GET, POST` | `/api/v1/custom-nodes` | List available reusable nodes or create one. |
| `GET, PATCH, DELETE` | `/api/v1/custom-nodes/{id}` | Read/edit/delete a reusable node according to ownership. |

The subscription response retains Toolbox's `ProxySubscribe` shape:
`id`, `userId`, `url`, `remark`, `logLevel`, `subscribeUrl`,
`subscribeItems`, `ruleList`, `useSystemRuleList`, `group`,
`useSystemGroup`, `filter`, `useSystemFilter`, `servers`, `customConfig`,
`useSystemCustomConfig`, `dnsConfig`, `useSystemDnsConfig`,
`privateAccessConfig`, `authorizedUserIds`, `cacheTtlMinutes`,
`assignedCustomNodes`, `selectedCustomNodeIds`, `lastAccessAt`,
`createdAt`, and `updatedAt`. Full read responses also include
`creator:{id,name,email}`, `cachedNodeCount`, `accessCount`,
`canEdit`, `canDelete`, and `canManageAuthorization`. The last three
values come from the server ACL.

Nonempty `subscribeItems` takes priority over the old `subscribeUrl` field;
an empty list falls back to the old field for migrated records. Each item
has `id`, `type` (`url` or `raw`), `enabled`, `prefix`, and `remark`.
URL items carry `url` and optional `cacheTtlMinutes`, `fetchUa`, and `fetchMode`
(`auto` or `domestic-direct`); RAW items carry `content` and never fetch over HTTP.
Existing items without a type are URLs. The list preserves mixed source order. The old
`subscribeUrl` value remains readable and editable for existing rows.
`servers` remains the configuration's private inline JSONC nodes; global
nodes remain separately owned and assigned. Inline, enabled global, then
remote source nodes is the output order.

The five `useSystem*` flags are per-field selectors. When true, compilation
uses the converter's current recommendation for the requested output target.
When false, compilation uses the saved field; explicit empty rules/groups stay
empty. The shared editor clears inactive custom values when inheritance is on
and copies current defaults when inheritance is turned off.
`GET /api/v1/subscription-defaults` supplies the converter's generic display
values. Existing Server settings are materialized once by migration 0010 plus
`editor_migration::run`; see [shared editor architecture](subscription-editor-architecture.md).
The three debug POST routes accept JSON with the existing session cookie and
respond with `text/event-stream`. They emit `stage` events as work occurs,
followed by exactly one terminal `result` (success) or `error` (failure).
Every event uses SSE `data:` containing a JSON object. A `stage` object has
`type` and `status` (`running`, `ok`, `skipped`, or `error`), with actual source
IDs, cache state, HTTP status, count, or message when observed. The terminal
event contains the complete debug DTO, including `ok`, `elapsedMs`, and the
accumulated `stages` and `diagnostics`; consumers replace the incremental
stage list with this final list. End of stream before a terminal event is a
failure, except when the caller deliberately aborts. Aborting the request
cancels the server work and any in-progress source fetch. Authentication,
subscription ACL, and malformed request errors occur before the stream opens
and retain their normal HTTP JSON error status.

Draft debugging accepts `{draft,target,subscriptionId?}`. For an existing
subscription, `subscriptionId` grants access to its assigned nodes through the
subscription ACL; a new draft may use only globally authorized nodes. It
accepts the full editable config and selected global node
IDs. It must use the same source fetch/cache and conversion service as preview
and public rendering, with any debug-only network behavior identified in the
response; it must not silently load the saved row in place of the submitted
draft.

Saved source debugging accepts
`{mode:"production",url:"https://example.com/nodes",subscriptionId,sourceIndex}`.
`url` is required by the request DTO but ignored in production mode. The
server enforces the subscription ACL and uses that saved source's URL, UA, fetch
mode, TTL, and prefix; unsaved request fields cannot override them. Unsaved
source debugging uses `{mode:"bypass-cache",url,ua?,fetchMode?,cacheTtlMinutes?,prefix?}`.
Both modes are read-only and never store fetched text. The response contains
`ok`, `status` (an actual HTTP response status, or `null` when no HTTP response
was received or a cache snapshot was used), `cacheState`, safe
`responseHeaders` (`content-type`, `etag`, `last-modified`, `cache-control`),
`raw` (at most 65,536 characters) with `rawTruncated`, `decodedText` (the
pre-parse base64-decoded text, when present, at most 65,536 characters) with
`decodedTextTruncated`, `decoded` (parsed node objects), node summaries,
elapsed time, and diagnostics. A non-200 or 200
response without usable nodes can be inspected but is never stored in the
formal source cache. Other formats have empty `decodedText`; their parsed
nodes remain in `decoded`.
Saved target debugging accepts `{target}` at
`/api/v1/subscriptions/{id}/debug` and returns the same `ok`, `content`,
`stages`, `diagnostics`, `fieldDiffs`, and `nodeCount` shape as draft debug,
plus `decoded` (parsed JSON/YAML when applicable) and converter
`nodeOrigins`. It does not make a target HTTP request or run a proxy core, so
it does not invent response headers or runtime validation.
The server streams real source and compilation stages, then the complete DTO;
it does not start a proxy core or invent network evidence for target compilation.

## Public URLs and target mapping

Public subscription links use the saved `proxy_subscribes.url` token under
`/api/subscriptions/{url}/`. Sing-box links require the full version and consumer.

| Public path suffix | Converter target |
| --- | --- |
| `clash`, `clash-meta` | `clash`, `clash-meta` |
| `sing-box/1.11/openwrt` | `sing-box-openwrt` |
| `sing-box/1.11/windows`, `sing-box/1.11/macos` | `sing-box-windows`, `sing-box-macos` |
| `sing-box/1.12/openwrt` | `sing-box-v12-openwrt` |
| `sing-box/1.12/windows`, `sing-box/1.12/macos` | `sing-box-v12-windows`, `sing-box-v12-macos` |
| `sing-box/1.13/openwrt` | `sing-box-v13-openwrt` |
| `sing-box/1.13/windows`, `sing-box/1.13/macos` | `sing-box-v13-windows`, `sing-box-v13-macos` |
| `sing-box/1.14/openwrt` | `sing-box-v14-openwrt` |
| `sing-box/1.14/windows`, `sing-box/1.14/macos` | `sing-box-v14-windows`, `sing-box-v14-macos` |
| `xray`, `v2ray`, `clash-rs`, `dae` | Same-named converter target. |

Short sing-box versions, omitted consumers, internal target names, and the old
public subscription prefixes are not public URL aliases.

Keep `/api/proxy/sing-box/convert/rule` and its `/12`, `/13`, and `/14` variants for
the public rule conversion URLs embedded in generated configurations.

Sempre's remote subscription client also has a real consumer contract. It
fetches a manifest URL with `?target=<converter-format>`, requires a
`schema: 1`, `service: "sempre"`, `read_only: true` manifest, then fetches a
same-origin artifact and checks its SHA-256. The public manifest endpoint is
`/api/subscriptions/{url}/manifest`; its artifact URL uses
`/api/subscriptions/{url}/artifacts/{artifact_id}`. Both use the same current
compilation service. The manifest needs `profile` name/revision/time,
`target`, artifact URL/hash/node count/time, runtime output settings, and an
edit URL. This preserves the client contract without a user-facing publish or share
operation. Manifest creation must persist an internal immutable artifact
snapshot. Its URL must retrieve those exact bytes so a later save or source
refresh cannot change the SHA-256 before the client fetches it. A stable
direct public URL renders the latest saved input. A successful public compile
persists the same immutable artifact internally. Failed public compilation
serves the most recently successful artifact for the same subscription,
target, saved configuration revision, and enabled node contents
with `x-sempre-stale: true`; stale source-cache fallback or successful partial
output that omitted a failed source also sets this header. The boolean header
does not distinguish these cases; authenticated debug stage events and
diagnostics identify the failed source and cache state.
The manifest's
`read_only` key must use that exact snake-case wire name, as required by
`sempre-subscription::remote::Manifest`. Revision must be monotonic across
saves; a seconds-resolution timestamp alone is insufficient.

Artifact identity includes both the input snapshot hash and output content
hash. A content match reuses immutable bytes and ID, while a separate
`last_success_at` pointer tracks the most recently successful result for
last-known-good fallback within the same saved revision. Older immutable
artifact URLs still retrieve their original bytes. The saved subscription row and selected assignments
are read from one repeatable-read snapshot. Draft debug returns actual
fetch/rule-provider/compile stage evidence and never writes persistent source
snapshots or artifacts. Node preview and trace do not fetch rule providers.
An empty draft output remains `ok: true` because a direct-only configuration
can be intentional; debug reports `nodeCount: 0` and a warning to inspect
enabled sources, nodes, and filters when proxy nodes were expected.
When an enabled source fails, its fetch event and diagnostic retain the
`sourceId` while healthy sources, manual nodes, and assigned nodes continue.
If all enabled sources fail and no usable nodes remain, compilation fails;
public output uses the last successful artifact for the current saved revision
if one exists. An invalid
editor JSONC field still fails explicitly. With `useSystemRuleList` or
`useSystemGroup` false, an empty/null custom value falls back to the system
default, whereas an invalid nonempty JSONC value fails. An empty custom filter
intentionally stays empty. Rule-provider fetching races automatic and configured
domestic-direct routes when `DIRECT_PROXY_URL` exists, preferring a fresh
successful response over an older cached fallback.

## Known migration differences

| Area | Current behavior | Acceptance boundary |
| --- | --- | --- |
| Duplicate node names | Sempre adds ` (2)` suffixes; Toolbox could emit duplicate names. | Preserve this safety improvement; compare real reference output before claiming byte equality. |
| Remote Clash rules for sing-box | Rule-provider content is fetched through bounded cache and compiled into a fixed snapshot. | Valid sing-box output has precedence over preserving the old remote conversion URL byte-for-byte. |
| Password writes | New registrations and changes currently validate `String::len()`, so the 12–1024 limit is measured in UTF-8 bytes, despite the API error saying "characters". Existing shorter Toolbox passwords still verify. | Confirm policy and align the error wording before production data migration. |
| Access statistics | Access log details are retained for the configured window (90 days by default). `totalAccesses` is a durable cumulative count initialized from available logs during migration; `recentAccessTotal` counts retained details. Recent logs are paginated at at most 100 per page. "Today" starts at UTC midnight. | Logs deleted before migration cannot be inferred from the database. Import known historical log IDs separately when a verified source snapshot is available. |
| Existing Toolbox database | Fresh schema and local browser/public flows are implemented. | In-place migration and real production-row equivalence remain unaccepted. |

## Current network acceptance boundary

Remote subscription sources and rule providers use the selected HTTP client
route for DNS resolution and connections. The server validates HTTP(S) URLs,
rejects embedded URL credentials, and bounds redirects, time, and response
size; it does not perform a separate system-DNS public-IP check or pin the
destination IP. The `auto` route retains reqwest's system proxy behavior.
`domestic-direct` uses the configured `DIRECT_PROXY_URL` HTTP(S) proxy. In
either route, the deployment's DNS and egress policy is responsible for
blocking access to internal destinations. In particular, an HTTP CONNECT
proxy resolves the destination independently of the server process. A local
FakeIP DNS answer therefore does not by itself reject a source before the
configured proxy is contacted.
The operator reports that upstream dnsmasq already blocks internal-domain
mappings; this has not been verified by the server or this migration.

Public access logs use the socket peer IP by default. Set
`SEMPRE_TRUSTED_PROXY_IPS` to a comma-separated list of reverse-proxy IP
addresses to inspect `X-Forwarded-For` from right to left, skipping listed
trusted hops and using the first untrusted address. If that header is absent,
`X-Real-IP` is used instead, only from a listed peer. The reverse proxy must
overwrite or append these headers from its observed client connection.
Unlisted peers cannot choose their logged IP through request headers.

## Independent acceptance slices

1. **Schema and auth:** A fresh database reaches the canonical schema through
   explicit `sempre-server migrate`; login and `me` work. Existing Toolbox
   production database in-place migration is pending separate acceptance.
2. **Subscription CRUD and authorization:** Owner and authorized editor read
   and save; only owner changes authorized users or deletes; all original
   fields round-trip; saved public `url` stays fixed.
3. **Nodes and defaults:** Existing global node assignments and inline nodes
   render in the expected order; disabled assignments stay selectable; the
   five default selectors and `GET defaults` agree.
4. **Fetch and compile:** Source UA, fetch mode, TTL, stale-cache fallback,
   and enabled flags work through one compile service. Draft debug has no
   database write; saved preview and public output agree for the same target.
5. **Public compatibility:** Every path in the table returns the expected
   format and records access; a Sempre remote client accepts the manifest and
   hash-checked artifact for a saved subscription.

Focused verification uses the existing converter and server tests, affected
target Clippy, and the QA browser and public-route smoke checks. New test cases
and old production database compatibility code still require approval under
`AGENTS.md` section 1.3.
