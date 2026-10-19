import { Select } from '@acme/components'
import { useContext } from 'react'
import type { EditorSource } from './sources'
import { useEditorI18n } from './i18n'

import { SubscriptionProxyContext } from './subscription-proxies'

export function SourceProxySelect({ source, disabled, update, size }: { source: EditorSource; disabled?: boolean; update: (patch: Partial<EditorSource>) => void; size?: 'small' }) {
  const proxies = useContext(SubscriptionProxyContext)
  const { t } = useEditorI18n()
  if (proxies === undefined) return <Select size={size} disabled={disabled} value={source.fetchMode ?? 'auto'} options={[{ value: 'auto', label: t('source.modeAuto') }, { value: 'domestic-direct', label: t('source.modeDirect') }]} onChange={next => update({ fetchMode: next as EditorSource['fetchMode'] })} className="w-full" />
  const options = [{ value: '', label: t('source.direct') }, ...proxies.map(proxy => ({ value: proxy.id, label: `${proxy.name}${proxy.enabled && proxy.status === 'running' ? '' : ` · ${t(`source.proxy.${proxy.enabled ? proxy.status : 'stopped'}`)}`}` }))]
  if (source.proxyId && !proxies.some(proxy => proxy.id === source.proxyId)) options.push({ value: source.proxyId, label: t('source.proxyUnavailable') })
  return <Select aria-label={t('source.subscriptionProxy')} size={size} disabled={disabled} value={source.proxyId ?? ''} options={options} onChange={next => update({ proxyId: String(next) || null })} className="w-full" />
}
