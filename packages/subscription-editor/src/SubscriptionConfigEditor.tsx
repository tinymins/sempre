import { Form, Select, Tabs, TextArea } from '@acme/components'
import { useState, type ReactNode } from 'react'
import { updateDnsField } from './dns-model'
import { DnsEditor, type DnsConfigEditorProps } from './DnsEditor'
import { InheritedSection } from './InheritedSection'
import { ManualNodesEditor, type EditorNode } from './ManualNodesEditor'
import { PrivateAccessEditor, type PrivateAccessEditorProps } from './PrivateAccessEditor'
import { SourceListEditor } from './SourceListEditor'
import { useEditorI18n } from './i18n'
import { inheritedFields, type ConfigDefaults, type EditorDraft, type InheritedField } from './model'
import type { EditorSource } from './sources'
import { EditorLayoutProvider, type EditorLayout } from './layout'

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
  layout?: EditorLayout
  tabBarFooter?: ReactNode
  onDebugSource?: (source: EditorSource, index: number) => void
  extraTabs?: { key: string; label: string; children: ReactNode; before?: string }[]
  dnsOptions?: Pick<DnsConfigEditorProps, 'features' | 'systemDnsListenHostOptions' | 'checkFakeIpRange'>
  privateAccessOptions?: Pick<PrivateAccessEditorProps, 'renderTransport' | 'renderHomeNetwork'>
}

export function SubscriptionConfigEditor({ value, defaults, onChange: emitChange, nodes, features, protocolCount, readOnly, activeKey, onActiveKeyChange, basicExtension, sourceExtension, dnsPreviewControls, onDebugSource, extraTabs = [], dnsOptions, privateAccessOptions, layout = 'dialog', tabBarFooter }: Props) {
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
  ]
  for (const tab of extraTabs) {
    const index = tabs.findIndex(item => item.key === tab.before)
    tabs.splice(index === -1 ? tabs.length : index, 0, tab)
  }
  const page = layout === 'page'
  const requested = activeKey ?? localKey
  const current = tabs.some(tab => tab.key === requested) ? requested : 'basic'
  const extraTabActive = extraTabs.some(tab => tab.key === current)
  const inherited = (field: InheritedField) => <InheritedSection readOnly={readOnly} field={field} value={value[field]} inherited={value[inheritedFields[field].flag]} defaultValue={defaults?.[field]}
    onChange={next => onChange({ [field]: next })} onInheritanceChange={next => onChange({ [inheritedFields[field].flag]: next, [field]: next ? null : defaults?.[field] ?? '' })}>
    {field === 'dnsConfig' ? (raw, inherited) => <DnsEditor value={raw} defaults={defaults?.dnsConfig} readOnly={readOnly || inherited} onChange={dnsConfig => onChange({ dnsConfig })} {...dnsOptions}
      checkFakeIpRange={current === 'dnsConfig' && !inherited ? dnsOptions?.checkFakeIpRange : undefined}
      onUseRecommendedRange={readOnly || inherited ? undefined : (key, range) => onChange({ dnsConfig: updateDnsField(raw, key, range), useSystemDnsConfig: false })} /> : undefined}
  </InheritedSection>

  return <EditorLayoutProvider layout={layout}><div className={page ? '' : 'flex min-h-[24rem] min-w-0 flex-1 flex-col gap-5'}>
    <div className={page ? 'mb-3 shrink-0 overflow-x-auto pb-1' : 'shrink-0 overflow-x-auto'}><Tabs className={page ? 'min-w-[920px]' : undefined} items={tabs.map(({ key, label }) => ({ key, label: page ? <span className={`text-sm ${key === current ? 'font-medium' : 'font-normal'}`}>{label}</span> : label }))} type="segment" activeKey={current} onChange={key => { setLocalKey(key); onActiveKeyChange?.(key) }} /></div>
    {tabBarFooter}
    {current === 'dnsConfig' ? dnsPreviewControls : null}
    <fieldset disabled={readOnly} hidden={extraTabActive} className={extraTabActive ? 'hidden' : page ? `m-0 min-w-0 border-0 p-0 ${readOnly ? 'pointer-events-none opacity-80' : ''}` : 'm-0 flex min-h-0 min-w-0 flex-1 flex-col border-0 p-0'}>
      <div hidden={current !== 'basic'} className={current === 'basic' ? page ? '' : 'space-y-4' : 'hidden'}>
        {page ? <Form.Item label={t('editor.remark')}><TextArea aria-label={t('editor.remark')} rows={3} placeholder={t('editor.remarkPlaceholder')} value={value.remark ?? ''} onChange={event => onChange({ remark: event.target.value })} /></Form.Item> : <label className="block space-y-1 text-sm">{t('editor.remark')}<TextArea rows={3} value={value.remark ?? ''} onChange={event => onChange({ remark: event.target.value })} /></label>}
        {supports('logging.level') ? page ? <Form.Item label={t('editor.logLevel')} tooltip={t('editor.logLevelHelp')}><Select value={value.logLevel} options={['off', 'error', 'warn', 'info', 'debug'].map(level => ({ value: level, label: t(`editor.logLevel.${level}`) }))} onChange={level => onChange({ logLevel: level as EditorDraft['logLevel'] })} /></Form.Item> : <label className="block space-y-1 text-sm">{t('editor.logLevel')}<Select value={value.logLevel} options={['off', 'error', 'warn', 'info', 'debug'].map(level => ({ value: level, label: level }))} onChange={level => onChange({ logLevel: level as EditorDraft['logLevel'] })} className="w-full" /></label> : null}
        {basicExtension}
      </div>
      <div hidden={current !== 'sources'} className={current === 'sources' ? page ? '' : 'space-y-4' : 'hidden'}>
        {page ? <Form.Item label={t('editor.sourcesLabel')}><SourceListEditor readOnly={readOnly} value={value.subscribeItems} onChange={subscribeItems => onChange({ subscribeItems })} onDebug={onDebugSource} /></Form.Item> : <SourceListEditor readOnly={readOnly} value={value.subscribeItems} onChange={subscribeItems => onChange({ subscribeItems })} onDebug={onDebugSource} />}
        {inherited('filter')}{sourceExtension}
      </div>
      {(['ruleList', 'group', 'customConfig', 'dnsConfig'] as const).map(field => <div key={field} hidden={current !== field} className={current === field ? page ? '' : 'flex min-h-[20rem] flex-1 flex-col' : 'hidden'}>{inherited(field)}</div>)}
      <div hidden={current !== 'privateAccess'} className={current === 'privateAccess' ? '' : 'hidden'}>{page ? <Form.Item label={t('editor.privateLabel')}><PrivateAccessEditor readOnly={readOnly} value={value.privateAccessConfig ?? ''} onChange={privateAccessConfig => onChange({ privateAccessConfig })} {...privateAccessOptions} /></Form.Item> : <PrivateAccessEditor readOnly={readOnly} value={value.privateAccessConfig ?? ''} onChange={privateAccessConfig => onChange({ privateAccessConfig })} {...privateAccessOptions} />}</div>
      <div hidden={current !== 'servers'} className={current === 'servers' ? '' : 'hidden'}><ManualNodesEditor readOnly={readOnly} draft={value} nodes={nodes} update={onChange} /></div>
    </fieldset>
    {extraTabs.map(tab => <fieldset disabled={readOnly} key={tab.key} hidden={current !== tab.key} className={current === tab.key ? `m-0 min-h-0 min-w-0 flex-1 border-0 p-0 ${page && readOnly ? 'pointer-events-none opacity-80' : ''}` : 'hidden'}>{current === tab.key ? tab.children : null}</fieldset>)}
  </div></EditorLayoutProvider>
}
