# Shared configuration editor

The client advanced editor and Server configuration modal use
`@acme/subscription-editor` from `packages/subscription-editor`. Server has no
simple-mode switch. Client simple-mode entry points remain client-owned.

```text
ui/ProxySubscribeEditor ───────┐
                              ├─ @acme/subscription-editor ─ @acme/components
server-ui/SubscriptionEditor ──┘

client profile ─────────────────┐
                               ├─ sempre-converter ─ target output
server subscription adapter ───┘
```

| Owner | Responsibility |
| --- | --- |
| Shared editor | Tabs, ordered URL/RAW sources, filters, inheritance controls, rule/group editors, DNS, private access, manual nodes, JSONC field edits |
| Client adapter | Automatic saving, update schedule, runtime settings, local network inventory, tunnel and home-network integration |
| Server adapter | Explicit saving, authorization, revision conflicts, HTTP fetching/cache and publication |
| Converter | Canonical defaults, JSONC parsing, configuration semantics and target compilation |

Do not copy shared fields back into either application. Add portable fields to
the shared editor and keep local runtime or multi-user operations in the host
adapters. Shared controls use `@acme/components`; host styles must scan the shared
package's source directory.

The client passes the `page` layout and its existing message labels. Page layout
retains the client tab typography, save status position, compact source controls,
node multiselect and plain dark JSONC presentation. The default `dialog` layout
keeps Server modal sizing and presentation. Layout does not select a different
settings model or save policy; automatic and explicit saving remain host-owned.
Client saves retain empty RAW sources and their stored metadata; URL fetch
defaults apply only to URL sources.

## Settings and output

There is one set of settings plus generated output. Inherited fields contain no
inactive custom value. Enabling inheritance clears the custom value; disabling
it copies the currently displayed defaults for editing. In-memory edits remain
available when switching tabs or when a Server revision conflict prevents saving.
These are not a separate persisted draft model.

The converter supplies both the generic editor defaults and recommendations by
core. The client uses its selected core; Server compilation uses the requested
output target. Rule-provider serialization preserves declaration order.

Structured DNS and private-access edits modify individual JSONC paths. They retain
comments, format overrides, unedited peers/DNS entries and extension fields.
Malformed documents remain editable in the advanced text view, while structured
controls are disabled. Domain validation still belongs to the converter.

## Existing Server data

Run the existing `sempre-server migrate` command before starting the updated
Server. Migration `0010_shared_editor.sql` marks old rows; `editor_migration::run`
then materializes their effective historical settings in one transaction and marks
them as migrated. This includes old rules/groups where empty JSONC meant defaults.
Inactive custom values are discarded as requested. Malformed custom text is kept
for manual correction. There is no archive or ongoing legacy branch in compilation.

New rows use the converter defaults. Old default files exist only under
`src/editor_migration/` as historical migration input. URLs, owners, memberships,
node assignments and access records are not changed. Updated settings revisions
cause generated artifacts to be reconsidered by the existing publication flow.
The migration is idempotent and startup refuses unconverted rows.
