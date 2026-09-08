import { Tag, Tooltip } from '@acme/components'
import { useI18n } from '../lib/i18n'
import { privateAccessMode, privateAccessTone } from '../lib/privateAccess'
import type { PrivateAccessStatus } from '../lib/types'
import { modeLabel } from './PrivateAccessRuntimePanel'

export function PrivateAccessStatusTag({ status }: { status?: PrivateAccessStatus }) {
  const { t } = useI18n()
  const summary = privateAccessMode(status)
  if (!status || !summary) return null
  const label = modeLabel(summary, t)

  return <Tooltip placement="bottom-start" title={<div className="min-w-52 space-y-2 py-1">
    <p className="font-semibold">{t('privateAccess')}</p>
    {status.connectors.map((connector) => <div key={connector.tag} className="flex items-center justify-between gap-3">
      <span className="min-w-0 truncate font-mono">{connector.tag}</span>
      <span className="flex shrink-0 items-center gap-1 text-[var(--muted)]">
        <span>{modeLabel(connector.mode, t)}</span>
        {connector.matched_network ? <><span>·</span><span>{connector.matched_network}</span></> : null}
      </span>
    </div>)}
  </div>}>
    <span aria-label={`${t('privateAccess')}: ${label}`} className="inline-flex cursor-help" tabIndex={0}>
      <Tag color={privateAccessTone(summary)}>{label}</Tag>
    </span>
  </Tooltip>
}
