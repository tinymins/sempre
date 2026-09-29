import { CodeEditor, Input, InputNumber, Select, Tabs } from '@acme/components'
import { findNodeAtLocation, parse, parseTree, type ParseError } from 'jsonc-parser'
import { useState } from 'react'
import { editJsonc, objectAt, readJsoncObject } from './jsonc-edit'
import { useI18n } from '../i18n/provider'
import type { MessageKey } from '../i18n/zh-CN'

interface Props {
  value: string | null
  onChange: (next: string | null) => void
  readOnly?: boolean
  fillHeight?: boolean
  onInvalidChange?: (invalid: boolean) => void
}

const overrideTabs = [
  { key: 'clash', label: 'Clash' },
  { key: 'clashMeta', label: 'Clash Meta' },
  { key: 'singbox', label: 'Sing-box v1.11' },
  { key: 'singboxV12', label: 'Sing-box v1.12+' },
] as const

type OverrideKey = typeof overrideTabs[number]['key']

function OverrideEditor({ name, value, onChange, onInvalidChange, readOnly, fillHeight }: { name: OverrideKey; value: string | null; onChange: Props['onChange']; onInvalidChange?: Props['onInvalidChange']; readOnly: boolean; fillHeight: boolean }) {
  const { t } = useI18n()
  const [activeRange, setActiveRange] = useState<{ source: string; offset: number; length: number } | null>(null)
  const [error, setError] = useState('')
  const source = value?.trim() ? value : '{}'
  const tree = parseTree(source)
  const node = tree ? findNodeAtLocation(tree, ['overrides', name]) : undefined
  const range = activeRange?.source === source ? activeRange : node ? { offset: node.offset, length: node.length } : null
  const text = range ? source.slice(range.offset, range.offset + range.length) : ''
  const update = (next: string) => {
    if (!next.trim()) {
      onChange(editJsonc(value, ['overrides', name], undefined))
      setActiveRange(null)
      setError('')
      onInvalidChange?.(false)
      return
    }
    let base = source
    let target = range
    if (!target) {
      base = editJsonc(value, ['overrides', name], {})
      const insertedTree = parseTree(base)
      const inserted = insertedTree ? findNodeAtLocation(insertedTree, ['overrides', name]) : undefined
      if (!inserted) return
      target = { offset: inserted.offset, length: inserted.length }
    }
    const nextSource = base.slice(0, target.offset) + next + base.slice(target.offset + target.length)
    onChange(nextSource)
    setActiveRange({ source: nextSource, offset: target.offset, length: next.length })
    const errors: ParseError[] = []
    const parsed: unknown = parse(next, errors, { allowTrailingComma: true })
    if (errors.length || !parsed || Array.isArray(parsed) || typeof parsed !== 'object') {
      setError(t('dns.invalidOverride'))
      onInvalidChange?.(true)
      return
    }
    setError('')
    onInvalidChange?.(false)
  }
  return <div className={fillHeight ? 'flex min-h-[18rem] flex-1 flex-col gap-2' : 'space-y-2'}>
    <p className="text-xs text-[var(--muted)]">{t('dns.overrideHint')}</p>
    {error ? <p role="alert" className="text-sm text-red-600">{error}</p> : null}
    <div className={fillHeight ? 'min-h-[16rem] flex-1' : ''}><CodeEditor ariaLabel={`${overrideTabs.find((tab) => tab.key === name)?.label} DNS`} height={fillHeight ? '100%' : 420} value={text} readOnly={readOnly} onChange={update} /></div>
  </div>
}

const stringFields: [string, MessageKey][] = [
  ['localDns', 'dns.localAddress'], ['bootstrapDns', 'dns.bootstrapAddress'],
  ['remoteDns', 'dns.remoteAddress'], ['fakeipIpv4Range', 'dns.fakeIpv4'],
  ['fakeipIpv6Range', 'dns.fakeIpv6'], ['clashApiSecret', 'dns.apiSecret'],
  ['clashApiUiPath', 'dns.apiUiPath'],
]

const numberFields: [string, MessageKey][] = [
  ['localDnsPort', 'dns.localPort'], ['dnsListenPort', 'dns.listenPort'],
  ['tproxyPort', 'dns.tproxyPort'], ['bootstrapDnsPort', 'dns.bootstrapPort'],
  ['remoteDnsPort', 'dns.remotePort'], ['fakeipTtl', 'dns.fakeTtl'],
  ['clashApiPort', 'dns.apiPort'],
]

const booleanFields: [string, MessageKey][] = [
  ['fakeipEnabled', 'dns.fakeEnabled'], ['rejectHttps', 'dns.rejectHttps'],
  ['cnDomainLocalDns', 'dns.cnDomain'], ['cnIpLocalDns', 'dns.cnIp'],
  ['preferIpv4', 'dns.preferIpv4'],
]

const groups: { title: MessageKey; fields: string[] }[] = [
  { title: 'dns.localSection', fields: ['localDnsTransport', 'localDns', 'localDnsPort', 'dnsListenPort', 'tproxyPort'] },
  { title: 'dns.upstreamSection', fields: ['bootstrapDns', 'bootstrapDnsPort', 'remoteDns', 'remoteDnsPort'] },
  { title: 'dns.fakeSection', fields: ['fakeipEnabled', 'fakeipIpv4Range', 'fakeipIpv6Range', 'fakeipTtl'] },
  { title: 'dns.rulesSection', fields: ['rejectHttps', 'cnDomainLocalDns', 'cnIpLocalDns', 'preferIpv4'] },
  { title: 'dns.clashSection', fields: ['clashApiSecret', 'clashApiUiPath', 'clashApiPort'] },
]

export function DnsConfigEditor({ value, onChange, readOnly = false, onInvalidChange, fillHeight = false }: Props) {
  const { t } = useI18n()
  const booleanOptions = [
    { value: 'unset', label: t('dns.unset') },
    { value: 'true', label: t('dns.enabled') },
    { value: 'false', label: t('dns.disabled') },
  ]
  const [tab, setTab] = useState<string>('shared')
  const [invalidOverrideTab, setInvalidOverrideTab] = useState<OverrideKey | null>(null)
  const [advancedInvalid, setAdvancedInvalid] = useState(false)
  const [advancedRevision, setAdvancedRevision] = useState(0)
  const { object, error } = readJsoncObject(value)
  const shapeError = object && object.shared !== undefined && (object.shared === null || Array.isArray(object.shared) || typeof object.shared !== 'object')
    ? t('dns.invalidShared') : null
  const issue = error ? t('editor.invalidObject') : shapeError
  const shared = object ? objectAt(object, 'shared') : {}
  const set = (key: string, next: unknown) => {
    if (issue || readOnly) return
    onChange(editJsonc(value, ['shared', key], next))
  }

  return (
    <div className={fillHeight && tab !== 'shared' ? 'flex min-h-[20rem] flex-1 flex-col gap-4' : 'space-y-4'}>
      <p className="text-xs text-[var(--muted)]">{t('dns.intro')}</p>
      {issue ? <p role="alert" className="text-sm text-red-600">{issue}</p> : null}
      <Tabs type="segment" activeKey={tab} onChange={setTab} items={[{ key: 'shared', label: t('dns.shared') }, ...overrideTabs]} />
      {invalidOverrideTab ? <p role="alert" className="text-xs text-red-600">{t('dns.fixOverride')}</p> : null}
      {!advancedInvalid && !shapeError ? overrideTabs.map(({ key }) => <div key={key} className={tab === key ? fillHeight ? 'flex min-h-[18rem] flex-1 flex-col' : '' : 'hidden'}>
        <OverrideEditor key={advancedRevision} name={key} value={value} onChange={onChange} readOnly={readOnly || Boolean(issue) && invalidOverrideTab !== key} fillHeight={fillHeight} onInvalidChange={(invalid) => { setInvalidOverrideTab(invalid ? key : null); onInvalidChange?.(invalid) }} />
      </div>) : null}
      {tab === 'shared' ? groups.map((group) => <section key={group.title} className="space-y-3">
        <h3 className="border-b border-[var(--border)] pb-1 text-sm font-medium">{t(group.title)}</h3>
        <div className="grid gap-3 md:grid-cols-2">{group.fields.map((key) => {
          if (key === 'localDnsTransport') return <label key={key} className="space-y-1 text-sm">{t('dns.transport')}
            <Select value={typeof shared.localDnsTransport === 'string' ? shared.localDnsTransport : 'unset'} disabled={readOnly || Boolean(issue)} options={[{ value: 'unset', label: t('dns.unset') }, { value: 'udp', label: 'UDP' }, { value: 'tls', label: 'TLS' }, { value: 'system', label: t('dns.system') }]} onChange={(next) => set('localDnsTransport', next === 'unset' ? undefined : next)} className="w-full" />
          </label>
          const stringLabel = stringFields.find(([field]) => field === key)?.[1]
          if (stringLabel) return <label key={key} className="space-y-1 text-sm">{t(stringLabel)}
            <Input value={typeof shared[key] === 'string' ? shared[key] : ''} disabled={readOnly || Boolean(issue)} onChange={(event) => set(key, event.target.value || undefined)} />
          </label>
          const numberLabel = numberFields.find(([field]) => field === key)?.[1]
          if (numberLabel) return <label key={key} className="space-y-1 text-sm">{t(numberLabel)}
            <InputNumber value={typeof shared[key] === 'number' ? shared[key] : null} min={key === 'fakeipTtl' ? 0 : 1} max={key === 'fakeipTtl' ? undefined : 65535} disabled={readOnly || Boolean(issue)} onChange={(next) => set(key, next ?? undefined)} className="w-full" />
          </label>
          const booleanLabel = booleanFields.find(([field]) => field === key)?.[1]
          return booleanLabel ? <label key={key} className="space-y-1 text-sm">{t(booleanLabel)}
            <Select value={typeof shared[key] === 'boolean' ? String(shared[key]) : 'unset'} disabled={readOnly || Boolean(issue)} options={booleanOptions} onChange={(next) => set(key, next === 'unset' ? undefined : next === 'true')} className="w-full" />
          </label> : null
        })}</div>
      </section>) : null}
      <label className="block space-y-1 text-sm">{t('filter.advanced')}
        <CodeEditor height={280} value={value ?? ''} readOnly={readOnly} ariaLabel={t('filter.advanced')} onChange={(text) => {
          const next = text || null
          const invalid = Boolean(readJsoncObject(next).error)
          setInvalidOverrideTab(null)
          setAdvancedInvalid(invalid)
          setAdvancedRevision((current) => current + 1)
          onInvalidChange?.(invalid)
          onChange(next)
        }} />
      </label>
      {object && 'overrides' in object ? <p className="text-xs text-[var(--muted)]">{t('dns.fallbackHint')}</p> : null}
    </div>
  )
}
