import { Button, CodeEditor, Input, InputNumber, Select, TextArea } from '@acme/components'
import { useState } from 'react'
import { editJsonc, objectAt, readJsoncObject, type JsonObject } from './jsonc-edit'
import { useI18n } from '../i18n/provider'

interface Props {
  value: string | null
  onChange: (next: string | null) => void
}

const types = ['wireguard', 'tailscale', 'outbound', 'v2ray', 'xray', 'vmess', 'vless', 'trojan', 'socks', 'socks5', 'http', 'ssh', 'hysteria2', 'tuic', 'anytls']
function arrayAt(value: unknown): unknown[] { return Array.isArray(value) ? value : [] }
function stringAt(object: JsonObject, key: string): string { return typeof object[key] === 'string' ? object[key] as string : '' }
function numberAt(object: JsonObject, key: string): number | null { return typeof object[key] === 'number' ? object[key] as number : null }
function lines(value: unknown): string { return arrayAt(value).filter((item): item is string => typeof item === 'string').join('\n') }
function parseLines(value: string, cidr = false): string[] {
  return value.split(/[\n,]/).map((item) => item.trim()).filter(Boolean)
    .map((item) => cidr && !item.includes('/') ? `${item}/${item.includes(':') ? 128 : 32}` : item)
}
function sameLines(left: string[], right: string[]): boolean {
  return left.length === right.length && left.every((item, index) => item === right[index])
}

function ListInput({ value, onChange, disabled, cidr = false }: { value: unknown; onChange: (next: string[]) => void; disabled: boolean; cidr?: boolean }) {
  const [editing, setEditing] = useState<{ text: string; emitted: string[] } | null>(null)
  const current = arrayAt(value).filter((item): item is string => typeof item === 'string')
  return <TextArea rows={2} value={editing && sameLines(current, editing.emitted) ? editing.text : lines(value)} disabled={disabled}
    onChange={(event) => {
      const text = event.target.value
      const emitted = parseLines(text, cidr)
      setEditing({ text, emitted })
      onChange(emitted)
    }} onBlur={() => setEditing(null)} />
}

export function PrivateAccessEditor({ value, onChange }: Props) {
  const { t, number } = useI18n()
  const enabledOptions = [
    { value: 'true', label: t('dns.enabled') },
    { value: 'false', label: t('dns.disabled') },
  ]
  const { object, error } = readJsoncObject(value)
  const shapeError = object && object.connectors !== undefined && !Array.isArray(object.connectors)
    ? t('private.invalid') : null
  const issue = error ? t('editor.invalidObject') : shapeError
  const connectors = object ? arrayAt(object.connectors) : []
  const set = (path: (string | number)[], next: unknown) => { if (!issue) onChange(editJsonc(value, path, next)) }
  const add = () => {
    const next = { enabled: true, tag: `private-access-${connectors.length + 1}`, type: 'wireguard', endpoint: { address: [], privateKey: '', peers: [{ persistentKeepaliveInterval: 25 }] } }
    set(['connectors', -1], next)
  }
  const remove = (index: number) => set(['connectors', index], undefined)

  return (
    <div className="space-y-4">
      <p className="text-xs text-[var(--muted)]">{t('private.intro')}</p>
      {issue ? <p role="alert" className="text-sm text-red-600">{issue}</p> : null}
      <label className="block space-y-1 text-sm">{t('private.enabled')}
        <Select value={object?.enabled === true ? 'true' : 'false'} disabled={Boolean(issue)} options={enabledOptions} onChange={(next) => set(['enabled'], next === 'true')} className="w-full" />
      </label>
      {connectors.map((raw, index) => {
        const connector = raw && typeof raw === 'object' && !Array.isArray(raw) ? raw as JsonObject : {}
        const kind = stringAt(connector, 'type')
        const endpoint = objectAt(connector, 'endpoint')
        const peer = arrayAt(endpoint.peers)[0] as JsonObject | undefined
        const outbound = objectAt(connector, 'outbound')
        const routes = objectAt(connector, 'routes')
        const dns = arrayAt(connector.dns)[0] as JsonObject | undefined
        const base = ['connectors', index]
        const setDns = (key: string, next: unknown) => {
          if (dns) {
            const updated = { ...dns, [key]: next }
            const suffixes = updated.domainSuffixes ?? updated.domain_suffix
            const port = updated.serverPort ?? updated.server_port ?? 53
            const extras = Object.keys(updated).some((field) => !['tag', 'domainSuffixes', 'domain_suffix', 'server', 'serverPort', 'server_port'].includes(field))
            const defaultTag = `${stringAt(connector, 'tag') || 'private-access'}-dns`
            const customTag = typeof updated.tag === 'string' && updated.tag.trim() && updated.tag !== defaultTag
            if (!updated.server && !arrayAt(suffixes).length && !(typeof suffixes === 'string' && suffixes.trim()) && port === 53 && !extras && !customTag) {
              set([...base, 'dns', ...(arrayAt(connector.dns).length > 1 ? [0] : [])], undefined)
            }
            else set([...base, 'dns', 0, key], next)
            return
          }
          if (next === '' || next === null || next === undefined || (Array.isArray(next) && next.length === 0) || next === 53) return
          set([...base, 'dns'], [{ tag: `${stringAt(connector, 'tag') || 'private-access'}-dns`, domainSuffixes: [], server: '', serverPort: 53, [key]: next }])
        }
        return <section key={index} className="space-y-3 rounded-lg border border-[var(--border)] p-3">
          <div className="flex items-center justify-between"><strong className="text-sm">{t('private.connector', { index: number(index + 1) })}</strong><Button size="small" danger disabled={Boolean(issue)} onClick={() => remove(index)}>{t('private.remove')}</Button></div>
          {!types.includes(kind) ? <p className="text-xs text-amber-700">{t('private.unsupported', { type: kind || t('dns.unset') })}</p> : null}
          <div className="grid gap-3 md:grid-cols-2">
            <label className="space-y-1 text-sm">{t('private.state')}<Select value={connector.enabled === false ? 'false' : 'true'} disabled={Boolean(issue)} options={enabledOptions} onChange={(next) => set([...base, 'enabled'], next === 'true')} className="w-full" /></label>
            <label className="space-y-1 text-sm">{t('private.tag')}<Input value={stringAt(connector, 'tag')} disabled={Boolean(issue)} onChange={(event) => set([...base, 'tag'], event.target.value)} /></label>
            <label className="space-y-1 text-sm">{t('private.protocol')}<Select value={kind || 'wireguard'} disabled={Boolean(issue)} options={types.map((item) => ({ value: item, label: item }))} onChange={(next) => set([...base, 'type'], next)} className="w-full" /></label>
          </div>
          {kind === 'wireguard' ? <div className="grid gap-3 md:grid-cols-2">
            <label className="space-y-1 text-sm">{t('private.localAddress')}<ListInput value={endpoint.address} disabled={Boolean(issue)} cidr onChange={(next) => set([...base, 'endpoint', 'address'], next)} /></label>
            <label className="space-y-1 text-sm">{t('private.privateKey')}<Input value={stringAt(endpoint, 'privateKey') || stringAt(endpoint, 'private_key')} disabled={Boolean(issue)} onChange={(event) => set([...base, 'endpoint', endpoint.private_key !== undefined ? 'private_key' : 'privateKey'], event.target.value)} /></label>
            <label className="space-y-1 text-sm">{t('private.peerAddress')}<Input value={peer ? stringAt(peer, 'address') : ''} disabled={Boolean(issue)} onChange={(event) => set([...base, 'endpoint', 'peers', 0, 'address'], event.target.value)} /></label>
            <label className="space-y-1 text-sm">{t('private.peerPort')}<InputNumber value={peer ? numberAt(peer, 'port') : null} min={1} max={65535} disabled={Boolean(issue)} onChange={(next) => set([...base, 'endpoint', 'peers', 0, 'port'], next ?? undefined)} className="w-full" /></label>
            <label className="space-y-1 text-sm">{t('private.publicKey')}<Input value={peer ? stringAt(peer, 'publicKey') || stringAt(peer, 'public_key') : ''} disabled={Boolean(issue)} onChange={(event) => set([...base, 'endpoint', 'peers', 0, peer?.public_key !== undefined ? 'public_key' : 'publicKey'], event.target.value)} /></label>
            <label className="space-y-1 text-sm">{t('private.preShared')}<Input value={peer ? stringAt(peer, 'preSharedKey') || stringAt(peer, 'pre_shared_key') : ''} disabled={Boolean(issue)} onChange={(event) => set([...base, 'endpoint', 'peers', 0, peer?.pre_shared_key !== undefined ? 'pre_shared_key' : 'preSharedKey'], event.target.value)} /></label>
            <label className="space-y-1 text-sm">{t('private.keepalive')}<InputNumber value={peer ? numberAt(peer, 'persistentKeepaliveInterval') ?? numberAt(peer, 'persistent_keepalive_interval') : null} min={0} disabled={Boolean(issue)} onChange={(next) => set([...base, 'endpoint', 'peers', 0, peer?.persistent_keepalive_interval !== undefined ? 'persistent_keepalive_interval' : 'persistentKeepaliveInterval'], next ?? undefined)} className="w-full" /></label>
            <label className="space-y-1 text-sm">{t('private.allowedIps')}<ListInput value={peer?.allowedIps ?? peer?.allowed_ips} disabled={Boolean(issue)} cidr onChange={(next) => set([...base, 'endpoint', 'peers', 0, peer?.allowed_ips !== undefined ? 'allowed_ips' : 'allowedIps'], next)} /></label>
          </div> : kind === 'tailscale' ? <p className="text-xs text-[var(--muted)]">{t('private.tailscaleHint')}</p> : types.includes(kind) ? <div className="grid gap-3 md:grid-cols-2">
            <label className="space-y-1 text-sm">{t('private.server')}<Input value={stringAt(outbound, 'server')} disabled={Boolean(issue)} onChange={(event) => set([...base, 'outbound', 'server'], event.target.value)} /></label>
            <label className="space-y-1 text-sm">{t('private.port')}<InputNumber value={numberAt(outbound, 'server_port') ?? numberAt(outbound, 'serverPort')} min={1} max={65535} disabled={Boolean(issue)} onChange={(next) => set([...base, 'outbound', outbound.serverPort !== undefined ? 'serverPort' : 'server_port'], next ?? undefined)} className="w-full" /></label>
            <label className="space-y-1 text-sm">{t('private.uuid')}<Input value={stringAt(outbound, 'uuid')} disabled={Boolean(issue)} onChange={(event) => set([...base, 'outbound', 'uuid'], event.target.value || undefined)} /></label>
            <label className="space-y-1 text-sm">{t('private.username')}<Input value={stringAt(outbound, 'username')} disabled={Boolean(issue)} onChange={(event) => set([...base, 'outbound', 'username'], event.target.value || undefined)} /></label>
            <label className="space-y-1 text-sm">{t('private.password')}<Input value={stringAt(outbound, 'password')} disabled={Boolean(issue)} onChange={(event) => set([...base, 'outbound', 'password'], event.target.value || undefined)} /></label>
          </div> : null}
          <div className="grid gap-3 md:grid-cols-2">
            <label className="space-y-1 text-sm">{t('private.routeCidrs')}<ListInput value={routes.ipCidrs} disabled={Boolean(issue)} cidr onChange={(next) => set([...base, 'routes', 'ipCidrs'], next)} /></label>
            <label className="space-y-1 text-sm">{t('private.routeSuffixes')}<ListInput value={routes.domainSuffixes} disabled={Boolean(issue)} onChange={(next) => set([...base, 'routes', 'domainSuffixes'], next)} /></label>
            <label className="space-y-1 text-sm">{t('private.dnsServer')}<Input value={dns ? stringAt(dns, 'server') : ''} disabled={Boolean(issue)} onChange={(event) => setDns('server', event.target.value)} /></label>
            <label className="space-y-1 text-sm">{t('private.dnsSuffixes')}<ListInput value={dns?.domainSuffixes ?? dns?.domain_suffix} disabled={Boolean(issue)} onChange={(next) => setDns(dns?.domain_suffix !== undefined ? 'domain_suffix' : 'domainSuffixes', next)} /></label>
            <label className="space-y-1 text-sm">{t('private.dnsPort')}<InputNumber value={dns ? numberAt(dns, 'serverPort') ?? numberAt(dns, 'server_port') ?? 53 : 53} min={1} max={65535} disabled={Boolean(issue)} onChange={(next) => setDns(dns?.server_port !== undefined ? 'server_port' : 'serverPort', next ?? undefined)} className="w-full" /></label>
          </div>
        </section>
      })}
      <Button disabled={Boolean(issue)} onClick={add}>{t('private.add')}</Button>
      <div className="space-y-1 text-sm">{t('filter.advanced')}
        <CodeEditor value={value ?? ''} height={320} ariaLabel={t('filter.advanced')} onChange={(next) => onChange(next || null)} />
      </div>
    </div>
  )
}
