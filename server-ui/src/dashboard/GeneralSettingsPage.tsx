import { Card } from '@acme/components'
import { useI18n } from '../i18n/provider'

export function GeneralSettingsPage() {
  const { t } = useI18n()
  return <section className="space-y-5">
    <Card><h2 className="font-semibold">{t('general.title')}</h2><p className="mt-1 text-sm text-[var(--muted)]">{t('general.intro')}</p></Card>
    <h1 className="text-xl font-semibold">{t('general.title')}</h1>
    <Card className="border-dashed py-10 text-center text-sm text-[var(--muted)]">{t('general.empty')}</Card>
  </section>
}
