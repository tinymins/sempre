import { Input, InputNumber, Select, TextArea } from '@acme/components'
import { editJsonc, objectAt, readJsoncObject } from './jsonc-edit'

interface Props {
  value: string | null
  onChange: (next: string | null) => void
  readOnly?: boolean
}

const stringFields = [
  ['localDns', '本地 DNS 地址'],
  ['bootstrapDns', '引导 DNS 地址'],
  ['remoteDns', '远程 DNS 地址'],
  ['fakeipIpv4Range', 'FakeIP IPv4 网段'],
  ['fakeipIpv6Range', 'FakeIP IPv6 网段'],
] as const

const numberFields = [
  ['localDnsPort', '本地 DNS 端口'],
  ['bootstrapDnsPort', '引导 DNS 端口'],
  ['remoteDnsPort', '远程 DNS 端口'],
  ['fakeipTtl', 'FakeIP TTL'],
] as const

const booleanFields = [
  ['fakeipEnabled', '启用 FakeIP'],
  ['rejectHttps', '拒绝 HTTPS DNS 查询'],
  ['cnDomainLocalDns', '中国域名使用本地 DNS'],
  ['cnIpLocalDns', '中国 IP 使用本地 DNS'],
  ['preferIpv4', '优先 IPv4'],
] as const

const booleanOptions = [
  { value: 'unset', label: '未设置（使用生成配置默认值）' },
  { value: 'true', label: '开启' },
  { value: 'false', label: '关闭' },
]

export function DnsConfigEditor({ value, onChange, readOnly = false }: Props) {
  const { object, error } = readJsoncObject(value)
  const shapeError = object && object.shared !== undefined && (object.shared === null || Array.isArray(object.shared) || typeof object.shared !== 'object')
    ? 'shared 必须是对象，请先在高级 JSONC 中修正。' : null
  const issue = error || shapeError
  const shared = object ? objectAt(object, 'shared') : {}
  const set = (key: string, next: unknown) => {
    if (issue || readOnly) return
    onChange(editJsonc(value, ['shared', key], next))
  }

  return (
    <div className="space-y-4">
      <p className="text-xs text-[var(--muted)]">这里编辑通用 DNS 字段。留空表示使用生成配置的默认值；各目标格式的覆盖配置和其他字段可在下方 JSONC 中编辑，并可用草稿调试核对输出。</p>
      {issue ? <p role="alert" className="text-sm text-red-600">{issue}</p> : null}
      <div className="grid gap-3 md:grid-cols-2">
        <label className="space-y-1 text-sm">本地 DNS 传输
          <Select value={typeof shared.localDnsTransport === 'string' ? shared.localDnsTransport : 'unset'} disabled={readOnly || Boolean(issue)} options={[{ value: 'unset', label: '未设置（使用生成配置默认值）' }, { value: 'udp', label: 'UDP' }, { value: 'tls', label: 'TLS' }, { value: 'system', label: '系统 DNS' }]} onChange={(next) => set('localDnsTransport', next === 'unset' ? undefined : next)} className="w-full" />
        </label>
        {stringFields.map(([key, label]) => <label key={key} className="space-y-1 text-sm">{label}
          <Input value={typeof shared[key] === 'string' ? shared[key] : ''} disabled={readOnly || Boolean(issue)} onChange={(event) => set(key, event.target.value || undefined)} />
        </label>)}
        {numberFields.map(([key, label]) => <label key={key} className="space-y-1 text-sm">{label}
          <InputNumber value={typeof shared[key] === 'number' ? shared[key] : null} min={key === 'fakeipTtl' ? 0 : 1} max={key === 'fakeipTtl' ? undefined : 65535} disabled={readOnly || Boolean(issue)} onChange={(next) => set(key, next ?? undefined)} className="w-full" />
        </label>)}
        {booleanFields.map(([key, label]) => <label key={key} className="space-y-1 text-sm">{label}
          <Select value={typeof shared[key] === 'boolean' ? String(shared[key]) : 'unset'} disabled={readOnly || Boolean(issue)} options={booleanOptions} onChange={(next) => set(key, next === 'unset' ? undefined : next === 'true')} className="w-full" />
        </label>)}
      </div>
      <label className="block space-y-1 text-sm">高级 JSONC 编辑
        <TextArea rows={12} value={value ?? ''} readOnly={readOnly} onChange={(event) => onChange(event.target.value || null)} className="font-mono text-xs" />
      </label>
      {object && 'overrides' in object ? <p className="text-xs text-[var(--muted)]">目标格式覆盖配置会完整替换该目标的 DNS 段。sing-box 1.12 及更新版本优先使用 singboxV12，回退到 singbox；Clash Meta 优先使用 clashMeta，回退到 clash。</p> : null}
    </div>
  )
}
