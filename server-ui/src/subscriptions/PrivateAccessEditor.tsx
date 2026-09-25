import { Button, Input, InputNumber, Select, TextArea } from '@acme/components'
import { editJsonc, objectAt, readJsoncObject, type JsonObject } from './jsonc-edit'

interface Props {
  value: string | null
  onChange: (next: string | null) => void
}

const types = ['wireguard', 'tailscale', 'outbound', 'v2ray', 'xray', 'vmess', 'vless', 'trojan', 'socks', 'socks5', 'http', 'ssh', 'hysteria2', 'tuic', 'anytls']
const enabledOptions = [
  { value: 'true', label: '启用' },
  { value: 'false', label: '停用' },
]

function arrayAt(value: unknown): unknown[] { return Array.isArray(value) ? value : [] }
function stringAt(object: JsonObject, key: string): string { return typeof object[key] === 'string' ? object[key] as string : '' }
function numberAt(object: JsonObject, key: string): number | null { return typeof object[key] === 'number' ? object[key] as number : null }
function lines(value: unknown): string { return arrayAt(value).filter((item): item is string => typeof item === 'string').join('\n') }
function parseLines(value: string): string[] { return value.split(/\r?\n/).map((item) => item.trim()).filter(Boolean) }

export function PrivateAccessEditor({ value, onChange }: Props) {
  const { object, error } = readJsoncObject(value)
  const shapeError = object && object.connectors !== undefined && !Array.isArray(object.connectors)
    ? 'connectors 必须是数组，请先在高级 JSONC 中修正。' : null
  const issue = error || shapeError
  const connectors = object ? arrayAt(object.connectors) : []
  const set = (path: (string | number)[], next: unknown) => { if (!issue) onChange(editJsonc(value, path, next)) }
  const add = () => {
    const next = { enabled: false, tag: `private-access-${connectors.length + 1}`, type: 'wireguard', endpoint: { address: [], privateKey: '', peers: [{}] } }
    set(['connectors', -1], next)
  }
  const remove = (index: number) => set(['connectors', index], undefined)

  return (
    <div className="space-y-4">
      <p className="text-xs text-[var(--muted)]">私网访问适用于 sing-box 1.12–1.14 目标格式。这里的结构化控件只编辑首个 WireGuard peer 和首条 DNS；其余条目仍保留并参与生成，可在高级 JSONC 中编辑。</p>
      {issue ? <p role="alert" className="text-sm text-red-600">{issue}</p> : null}
      <label className="block space-y-1 text-sm">私网访问
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
        return <section key={index} className="space-y-3 rounded-lg border border-[var(--border)] p-3">
          <div className="flex items-center justify-between"><strong className="text-sm">连接器 {index + 1}</strong><Button size="small" danger disabled={Boolean(issue)} onClick={() => remove(index)}>移除</Button></div>
          {!types.includes(kind) ? <p className="text-xs text-amber-700">类型 {kind || '未设置'} 当前不受目标格式支持；原始内容保留在 JSONC 中。</p> : null}
          <div className="grid gap-3 md:grid-cols-2">
            <label className="space-y-1 text-sm">状态<Select value={connector.enabled === false ? 'false' : 'true'} disabled={Boolean(issue)} options={enabledOptions} onChange={(next) => set([...base, 'enabled'], next === 'true')} className="w-full" /></label>
            <label className="space-y-1 text-sm">标签<Input value={stringAt(connector, 'tag')} disabled={Boolean(issue)} onChange={(event) => set([...base, 'tag'], event.target.value)} /></label>
            <label className="space-y-1 text-sm">协议<Select value={kind || 'wireguard'} disabled={Boolean(issue)} options={types.map((item) => ({ value: item, label: item }))} onChange={(next) => set([...base, 'type'], next)} className="w-full" /></label>
          </div>
          {kind === 'wireguard' ? <div className="grid gap-3 md:grid-cols-2">
            <label className="space-y-1 text-sm">本地地址（每行一个）<TextArea rows={2} value={lines(endpoint.address)} disabled={Boolean(issue)} onChange={(event) => set([...base, 'endpoint', 'address'], parseLines(event.target.value))} /></label>
            <label className="space-y-1 text-sm">私钥<Input value={stringAt(endpoint, 'privateKey') || stringAt(endpoint, 'private_key')} disabled={Boolean(issue)} onChange={(event) => set([...base, 'endpoint', endpoint.private_key !== undefined ? 'private_key' : 'privateKey'], event.target.value)} /></label>
            <label className="space-y-1 text-sm">对端地址<Input value={peer ? stringAt(peer, 'address') : ''} disabled={Boolean(issue)} onChange={(event) => set([...base, 'endpoint', 'peers', 0, 'address'], event.target.value)} /></label>
            <label className="space-y-1 text-sm">对端端口<InputNumber value={peer ? numberAt(peer, 'port') : null} min={1} max={65535} disabled={Boolean(issue)} onChange={(next) => set([...base, 'endpoint', 'peers', 0, 'port'], next ?? undefined)} className="w-full" /></label>
            <label className="space-y-1 text-sm">对端公钥<Input value={peer ? stringAt(peer, 'publicKey') || stringAt(peer, 'public_key') : ''} disabled={Boolean(issue)} onChange={(event) => set([...base, 'endpoint', 'peers', 0, peer?.public_key !== undefined ? 'public_key' : 'publicKey'], event.target.value)} /></label>
            <label className="space-y-1 text-sm">允许 IP（每行一个）<TextArea rows={2} value={peer ? lines(peer.allowedIps ?? peer.allowed_ips) : ''} disabled={Boolean(issue)} onChange={(event) => set([...base, 'endpoint', 'peers', 0, peer?.allowed_ips !== undefined ? 'allowed_ips' : 'allowedIps'], parseLines(event.target.value))} /></label>
          </div> : kind === 'tailscale' ? <p className="text-xs text-[var(--muted)]">Tailscale 的 endpoint 请在高级 JSONC 中编辑。</p> : types.includes(kind) ? <div className="grid gap-3 md:grid-cols-2">
            <label className="space-y-1 text-sm">服务器<Input value={stringAt(outbound, 'server')} disabled={Boolean(issue)} onChange={(event) => set([...base, 'outbound', 'server'], event.target.value)} /></label>
            <label className="space-y-1 text-sm">端口<InputNumber value={numberAt(outbound, 'server_port') ?? numberAt(outbound, 'serverPort')} min={1} max={65535} disabled={Boolean(issue)} onChange={(next) => set([...base, 'outbound', outbound.serverPort !== undefined ? 'serverPort' : 'server_port'], next ?? undefined)} className="w-full" /></label>
            <label className="space-y-1 text-sm">UUID（如协议需要）<Input value={stringAt(outbound, 'uuid')} disabled={Boolean(issue)} onChange={(event) => set([...base, 'outbound', 'uuid'], event.target.value || undefined)} /></label>
            <label className="space-y-1 text-sm">用户名（如协议需要）<Input value={stringAt(outbound, 'username')} disabled={Boolean(issue)} onChange={(event) => set([...base, 'outbound', 'username'], event.target.value || undefined)} /></label>
            <label className="space-y-1 text-sm">密码（如协议需要）<Input value={stringAt(outbound, 'password')} disabled={Boolean(issue)} onChange={(event) => set([...base, 'outbound', 'password'], event.target.value || undefined)} /></label>
          </div> : null}
          <div className="grid gap-3 md:grid-cols-2">
            <label className="space-y-1 text-sm">路由 IP CIDR（每行一个）<TextArea rows={2} value={lines(routes.ipCidrs)} disabled={Boolean(issue)} onChange={(event) => set([...base, 'routes', 'ipCidrs'], parseLines(event.target.value))} /></label>
            <label className="space-y-1 text-sm">路由域名后缀（每行一个）<TextArea rows={2} value={lines(routes.domainSuffixes)} disabled={Boolean(issue)} onChange={(event) => set([...base, 'routes', 'domainSuffixes'], parseLines(event.target.value))} /></label>
            <label className="space-y-1 text-sm">专用 DNS 服务器<Input value={dns ? stringAt(dns, 'server') : ''} disabled={Boolean(issue)} onChange={(event) => set([...base, 'dns', 0, 'server'], event.target.value)} /></label>
            <label className="space-y-1 text-sm">专用 DNS 端口<InputNumber value={dns ? numberAt(dns, 'serverPort') : null} min={1} max={65535} disabled={Boolean(issue)} onChange={(next) => set([...base, 'dns', 0, 'serverPort'], next ?? undefined)} className="w-full" /></label>
          </div>
        </section>
      })}
      <Button disabled={Boolean(issue)} onClick={add}>添加连接器（初始停用）</Button>
      <label className="block space-y-1 text-sm">高级 JSONC 编辑
        <TextArea rows={14} value={value ?? ''} onChange={(event) => onChange(event.target.value || null)} className="font-mono text-xs" />
      </label>
    </div>
  )
}
