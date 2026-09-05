import { Tag } from '@acme/components'
import { Network } from 'lucide-react'
import { useI18n } from '../lib/i18n'
import { privateAccessMode, privateAccessTone, type PrivateAccessMode } from '../lib/privateAccess'
import type { PrivateAccessStatus } from '../lib/types'

export function PrivateAccessRuntimePanel({ status }: { status?: PrivateAccessStatus }) {
  const { t } = useI18n()
  const summary = privateAccessMode(status)
  if (!status || !summary) return null
  return <div className="mx-4 mb-4 rounded-md border border-[var(--border)] bg-[var(--surface-subtle)] p-3 text-sm md:mx-5 md:mb-5">
    <div className="flex flex-wrap items-center gap-2"><Network size={16} className="text-emerald-600" /><span className="font-medium">{t('privateAccessAuto')}</span><Tag color={privateAccessTone(summary)}>{modeLabel(summary, t)}</Tag></div>
    <div className="mt-3 space-y-3">
      {status.connectors.map((connector) => <div key={connector.tag} className="grid gap-x-5 gap-y-2 border-t border-[var(--border)] pt-3 first:border-t-0 first:pt-0 sm:grid-cols-2 lg:grid-cols-4">
        <Info label={t('privateAccessConnector')} value={connector.tag} mono />
        <Info label={t('privateAccessPath')} value={modeLabel(connector.mode, t)} />
        <Info label={t('privateAccessInterface')} value={status.interface || '-'} mono />
        <Info label={t('privateAccessAddress')} value={status.interface_addresses.join(', ') || '-'} mono />
        <Info label={t('privateAccessHomeCidrs')} value={connector.home_networks.join(', ')} />
        <Info label={t('privateAccessMatch')} value={connector.matched_network || t('privateAccessNoMatch')} />
      </div>)}
    </div>
    {status.probe_error ? <p className="mt-3 break-words text-xs text-amber-700 dark:text-amber-300">{status.probe_error}</p> : null}
  </div>
}

function Info({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return <div className="min-w-0"><p className="text-xs text-[var(--muted)]">{label}</p><p className={`mt-1 break-words font-medium ${mono ? 'font-mono text-xs' : 'text-sm'}`}>{value}</p></div>
}

export function modeLabel(mode: PrivateAccessMode | 'direct' | 'wireguard' | 'unknown' | 'inactive', t: ReturnType<typeof useI18n>['t']) {
  if (mode === 'direct') return t('privateAccessDirect')
  if (mode === 'wireguard') return t('privateAccessWireGuard')
  if (mode === 'mixed') return t('privateAccessMixed')
  if (mode === 'inactive') return t('privateAccessInactive')
  return t('privateAccessUnknown')
}
