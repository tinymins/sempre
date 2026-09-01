import { RuntimeControlPanel } from '../components/RuntimeControlPanel'
import { PageTitle } from '../components/ui'
import { useI18n } from '../lib/i18n'

export function RuntimeStatus() {
  const { t } = useI18n()
  return <div className="space-y-5"><PageTitle title={t('navigationCoreStatus')} /><RuntimeControlPanel /></div>
}
