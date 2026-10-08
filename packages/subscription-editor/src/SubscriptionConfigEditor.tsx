import { Select, Tabs, TextArea } from '@acme/components'
import { useState, type ReactNode } from 'react'
import { DnsEditor, type DnsConfigEditorProps } from './DnsEditor'
import { InheritedSection } from './InheritedSection'
import { ManualNodesEditor, type EditorNode } from './ManualNodesEditor'
import { PrivateAccessEditor, type PrivateAccessEditorProps } from './PrivateAccessEditor'
import { SourceListEditor } from './SourceListEditor'
import { useEditorI18n } from './i18n'
import { inheritedFields, type ConfigDefaults, type EditorDraft, type InheritedField } from './model'
import type { EditorSource } from './sources'

interface Props {
  value: EditorDraft
  defaults: ConfigDefaults | null
  onChange: (patch: Partial<EditorDraft>) => void
  nodes: EditorNode[]
  features?: string[]
  protocolCount?: number
  readOnly?: boolean
  activeKey?: string
  onActiveKeyChange?: (key: string) => void
  basicExtension?: ReactNode
  sourceExtension?: ReactNode
  dnsPreviewControls?: ReactNode
  onDebugSource?: (source: EditorSource, index: number) => void
  extraTabs?: { key: string; label: string; children: ReactNode }[]
  dnsOptions?: Pick<DnsConfigEditorProps, 'features' | 'systemDnsListenHostOptions'>
  privateAccessOptions?: Pick<PrivateAccessEditorProps, 'renderTransport' | 'renderHomeNetwork'>
}

export function SubscriptionConfigEditor({ value, defaults, onChange: emitChange, nodes, features, protocolCount, readOnly, activeKey, onActiveKeyChange, basicExtension, sourceExtension, dnsPreviewControls, onDebugSource, extraTabs = [], dnsOptions, privateAccessOptions }: Props) {
  const { t } = useEditorI18n()
  const onChange = (patch: Partial<EditorDraft>) => { if (!readOnly) emitChange(patch) }
  const [localKey, setLocalKey] = useState('basic')
  const supports = (feature: string) => features === undefined || features.includes(feature)
  const tabs = [
    { key: 'basic', label: t('editor.tabBasic') },
    { key: 'sources', label: t('editor.tabSources') },
    ...(supports('routing.rule_providers') ? [{ key: 'ruleList', label: t('editor.tabRules') }] : []),
    ...(supports('routing.selector') || supports('routing.url_test') ? [{ key: 'group', label: t('editor.tabGroup') }] : []),
    ...(supports('routing.rules') ? [{ key: 'customConfig', label: t('editor.tabCustom') }] : []),
    ...(features === undefined || features.some(feature => feature.startsWith('dns.')) ? [{ key: 'dnsConfig', label: t('editor.tabDns') }] : []),
    ...(supports('private_access') ? [{ key: 'privateAccess', label: t('editor.tabPrivate') }] : []),
    ...(protocolCount === undefined || protocolCount > 0 ? [{ key: 'servers', label: t('editor.tabServers') }] : []),
    ...extraTabs,
  ]
  const requested = activeKey ?? localKey
  const current = tabs.some(tab => tab.key === requested) ? requested : 'basic'
  const extraTabActive = extraTabs.some(tab => tab.key === current)
  const inherited = (field: InheritedField) => <InheritedSection readOnly={readOnly} field={field} value={value[field]} inherited={value[inheritedFields[field].flag]} defaultValue={defaults?.[field]}
    onChange={next => onChange({ [field]: next })} onInheritanceChange={next => onChange({ [inheritedFields[field].flag]: next, [field]: next ? null : defaults?.[field] ?? '' })}>
    {field === 'dnsConfig' ? (raw, inherited) => <DnsEditor value={raw} defaults={defaults?.dnsConfig} readOnly={readOnly || inherited} onChange={dnsConfig => onChange({ dnsConfig })} {...dnsOptions} /> : undefined}
  </InheritedSection>

  return <div className="flex min-h-[24rem] min-w-0 flex-1 flex-col gap-5">
    <div className="shrink-0 overflow-x-auto"><Tabs items={tabs.map(({ key, label }) => ({ key, label }))} type="segment" activeKey={current} onChange={key => { setLocalKey(key); onActiveKeyChange?.(key) }} /></div>
    {current === 'dnsConfig' ? dnsPreviewControls : null}
    <fieldset disabled={readOnly} hidden={extraTabActive} className={extraTabActive ? 'hidden' : 'm-0 flex min-h-0 min-w-0 flex-1 flex-col border-0 p-0'}>
      <div hidden={current !== 'basic'} className={current === 'basic' ? 'space-y-4' : 'hidden'}>
        <label className="block space-y-1 text-sm">{t('editor.remark')}<TextArea rows={3} value={value.remark ?? ''} onChange={event => onChange({ remark: event.target.value })} /></label>
        {supports('logging.level') ? <label className="block space-y-1 text-sm">{t('editor.logLevel')}<Select value={value.logLevel} options={['off', 'error', 'warn', 'info', 'debug'].map(level => ({ value: level, label: level }))} onChange={level => onChange({ logLevel: level as EditorDraft['logLevel'] })} className="w-full" /></label> : null}
        {basicExtension}
      </div>
      <div hidden={current !== 'sources'} className={current === 'sources' ? 'space-y-4' : 'hidden'}>
        <SourceListEditor readOnly={readOnly} value={value.subscribeItems} onChange={subscribeItems => onChange({ subscribeItems })} onDebug={onDebugSource} />
        {inherited('filter')}{sourceExtension}
      </div>
      {(['ruleList', 'group', 'customConfig', 'dnsConfig'] as const).map(field => <div key={field} hidden={current !== field} className={current === field ? 'flex min-h-[20rem] flex-1 flex-col' : 'hidden'}>{inherited(field)}</div>)}
      <div hidden={current !== 'privateAccess'} className={current === 'privateAccess' ? '' : 'hidden'}><PrivateAccessEditor readOnly={readOnly} value={value.privateAccessConfig ?? ''} onChange={privateAccessConfig => onChange({ privateAccessConfig })} {...privateAccessOptions} /></div>
      <div hidden={current !== 'servers'} className={current === 'servers' ? '' : 'hidden'}><ManualNodesEditor readOnly={readOnly} draft={value} nodes={nodes} update={onChange} /></div>
    </fieldset>
    {extraTabs.map(tab => <div key={tab.key} hidden={current !== tab.key} className={current === tab.key ? 'min-h-0 min-w-0 flex-1' : 'hidden'}>{current === tab.key ? tab.children : null}</div>)}
  </div>
}
