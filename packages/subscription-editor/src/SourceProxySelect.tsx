import { Select } from '@acme/components'
import { useContext } from 'react'
import type { EditorSource } from './sources'
import { useEditorI18n } from './i18n'

import { SubscriptionProxyContext } from './subscription-proxies'

const DOMESTIC = 'domestic-direct'

export function SourceProxySelect({ source, disabled, update, size }: { source: EditorSource; disabled?: boolean; update: (patch: Partial<EditorSource>) => void; size?: 'small' }) {
  const proxies = useContext(SubscriptionProxyContext)
  const { t } = useEditorI18n()
  if (proxies === undefined) return <Select size={size} disabled={disabled} value={source.fetchMode ?? 'auto'} options={[{ value: 'auto', label: t('source.modeAuto') }, { value: 'domestic-direct', label: t('source.modeDirect') }]} onChange={next => update({ fetchMode: next as EditorSource['fetchMode'] })} className="w-full" />
  const domestic = source.fetchMode === 'domestic-direct' && !source.proxyId
  const options = [...(domestic ? [{ value: DOMESTIC, label: t('source.modeDirect') }] : []), { value: '', label: t('source.direct') }, ...proxies.map(proxy => ({ value: proxy.id, label: `${proxy.name}${proxy.enabled && proxy.status === 'running' ? '' : ` · ${t(`source.proxy.${proxy.enabled ? proxy.status : 'stopped'}`)}`}` }))]
  if (source.proxyId && !proxies.some(proxy => proxy.id === source.proxyId)) options.push({ value: source.proxyId, label: t('source.proxyUnavailable') })
  return <Select aria-label={t('source.subscriptionProxy')} size={size} disabled={disabled} value={domestic ? DOMESTIC : source.proxyId ?? ''} options={options} onChange={next => { if (next !== DOMESTIC) update({ proxyId: String(next) || null, ...(domestic ? { fetchMode: 'auto' as const } : {}) }) }} className="w-full" />
}
