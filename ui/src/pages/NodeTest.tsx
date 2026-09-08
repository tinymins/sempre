import { useI18n } from '../lib/i18n'
import { NodeTestPanel } from '../features/network/NodeTestPanel'

export function NodeTest() {
  const { t } = useI18n()
  return <div className="space-y-5">
    <div><h1 className="text-xl font-semibold">{t('nodeTest')}</h1><p className="mt-1 text-sm text-[var(--muted)]">{t('nodeTestDetail')}</p></div>
    <NodeTestPanel />
  </div>
}
