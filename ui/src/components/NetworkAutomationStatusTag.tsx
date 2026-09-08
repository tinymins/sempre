import { Tag, Tooltip } from '@acme/components'
import { useI18n } from '../lib/i18n'
import type { NetworkAutomationDisplayPath } from '../lib/networkAutomation'
import type { NetworkAutomationStatus } from '../lib/networkTypes'

export function NetworkAutomationStatusTag({ status, path, label }: { status?: NetworkAutomationStatus; path: NetworkAutomationDisplayPath; label: string }) {
  const { t } = useI18n()
  const detail = path === 'direct' ? t('publicDirectDetail') : path === 'proxy' ? t('publicProxyDetail') : null
  const tag = <span aria-label={`${t('publicTraffic')}: ${label}`} className={detail ? 'inline-flex cursor-help' : 'inline-flex'} tabIndex={detail ? 0 : undefined}>
    <Tag color={path === 'direct' ? 'green' : path === 'proxy' ? 'blue' : 'orange'}>{label}</Tag>
  </span>
  if (!detail) return tag

  return <Tooltip placement="bottom-start" title={<div className="min-w-64 space-y-2 py-1">
    <p className="font-semibold">{t('publicTraffic')} · {label}</p>
    <div className="flex items-center justify-between gap-3">
      <span className="text-[var(--muted)]">{t('currentNetwork')}</span>
      <span className="font-medium">{status?.network_name || t('unknownNetwork')}</span>
    </div>
    <p className="border-t border-[var(--border)] pt-2 leading-5 text-[var(--muted)]">{detail}</p>
  </div>}>{tag}</Tooltip>
}
