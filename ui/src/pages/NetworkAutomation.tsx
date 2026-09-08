import { NetworkAutomationPanel } from '../components/NetworkAutomationPanel'
import { PageTitle } from '../components/ui'
import { useI18n } from '../lib/i18n'

export function NetworkAutomation() {
  const { t } = useI18n()
  return <div className="space-y-5"><PageTitle title={t('networkAutomation')} /><NetworkAutomationPanel /></div>
}
