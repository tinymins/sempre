import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { CheckCircle2, Download, Power, RefreshCw, ServerCog, ShieldAlert } from 'lucide-react'
import { api } from '../../lib/api'
import { useI18n } from '../../lib/i18n'
import { useSession } from '../../lib/session'
import type { ServiceUpdateStatus, SystemStatus } from '../../lib/types'
import { Badge, Button, Card, ConfirmDialog, Spinner } from '../../components/ui'
import { ReleaseNotes } from './ReleaseNotes'

export function ServicePanel() {
  const { locale, t } = useI18n()
  const { session } = useSession()
  const queryClient = useQueryClient()
  const [notice, setNotice] = useState('')
  const [serviceConfirm, setServiceConfirm] = useState<'restart' | 'stop' | null>(null)
  const [serviceConfirmOpen, setServiceConfirmOpen] = useState(false)
  const system = useQuery({ queryKey: ['system'], queryFn: () => api<SystemStatus>(session!, '/system') })
  const update = useQuery({ queryKey: ['service', 'update'], queryFn: () => api<ServiceUpdateStatus>(session!, '/service/update'), enabled: false, retry: false })
  const upgrade = useMutation({
    mutationFn: () => api<{ status: string; update: ServiceUpdateStatus }>(session!, '/service/update', { method: 'POST' }),
    onSuccess: (result) => {
      queryClient.setQueryData(['service', 'update'], result.update)
      setNotice(t('serviceUpdateScheduled'))
    },
    onError: (error) => setNotice(error.message),
  })
  const serviceMutation = useMutation({
    mutationFn: (action: string) => api(session!, '/service/action', { method: 'POST', body: JSON.stringify({ action }) }),
    onSuccess: () => { setNotice(t('operationAccepted')); setServiceConfirmOpen(false) },
    onError: (error) => setNotice(error.message),
  })
  const serviceAvailable = system.data?.mode === 'system' && system.data.service !== 'not installed'
  const currentVersion = update.data?.current_version ?? system.data?.version ?? '-'
  const releaseHistory = (update.data?.release_history?.length ? update.data.release_history : update.data ? [{ version: update.data.latest_version, published_at: update.data.published_at, notes: update.data.release_notes }] : [])
    .map((release) => ({ ...release, notes: release.notes || t('noReleaseNotes') }))

  return <div className="space-y-5">
    {notice ? <div role="status" className="border-l-2 border-emerald-500 bg-emerald-500/8 px-3 py-2 text-sm">{notice}</div> : null}
    <Card className="p-4 md:p-5">
      <div className="mb-5 flex flex-wrap items-center justify-between gap-3"><div className="flex items-center gap-2"><ServerCog size={18} className="text-emerald-600" /><h2 className="text-sm font-semibold">{t('serviceUpdateTitle')}</h2></div><Button disabled={update.isFetching || upgrade.isPending} onClick={() => void update.refetch()}>{update.isFetching ? <Spinner /> : <RefreshCw size={16} />}{t('checkForUpdates')}</Button></div>
      <div className="grid gap-3 rounded-lg bg-[var(--surface-hover)] p-4 sm:grid-cols-2">
        <Version label={t('currentVersion')} value={currentVersion} />
        <Version label={t('latestVersion')} value={update.data?.latest_version ?? t('notChecked')} />
      </div>
      {update.isError ? <p role="alert" className="mt-4 text-sm text-red-600 dark:text-red-400">{update.error.message}</p> : null}
      {update.data ? <div className="mt-4 space-y-4">
        <div className="flex flex-wrap items-center gap-3"><Badge tone={update.data.update_available ? 'warning' : 'success'}>{update.data.update_available ? t('updateAvailable') : t('upToDate')}</Badge>{update.data.published_at ? <span className="text-xs text-[var(--muted)]">{new Date(update.data.published_at).toLocaleString()}</span> : null}</div>
        {update.data.update_available ? <><div><h3 className="mb-2 text-sm font-semibold">{t('releaseNotes')}</h3><ReleaseNotes releases={releaseHistory.length ? releaseHistory : [{ version: update.data.latest_version, published_at: update.data.published_at, notes: t('noReleaseNotes') }]} locale={locale} /></div><Button variant="primary" disabled={!serviceAvailable || upgrade.isPending} onClick={() => upgrade.mutate()}>{upgrade.isPending ? <Spinner /> : <Download size={16} />}{t('upgradeNow')}</Button>{!serviceAvailable ? <p className="text-xs text-[var(--muted)]">{t('systemServiceUpdateOnly')}</p> : null}</> : <div className="flex items-center gap-2 text-sm text-emerald-600 dark:text-emerald-400"><CheckCircle2 size={17} />{t('upToDateDetail')}</div>}
      </div> : null}
    </Card>
    <Card className="p-4 md:p-5">
      <div className="mb-5 flex items-center gap-2"><ShieldAlert size={18} className="text-emerald-600" /><h2 className="text-sm font-semibold">{t('systemServiceActions')}</h2></div>
      <div className="flex flex-wrap items-center justify-between gap-4"><div><Badge tone="danger">{t('dangerZone')}</Badge><p className="mt-2 max-w-2xl text-sm text-[var(--muted)]">{t('serviceRestartWarning')}</p></div><div className="flex gap-2"><Button disabled={!serviceAvailable || serviceMutation.isPending} onClick={() => { setServiceConfirm('restart'); setServiceConfirmOpen(true) }}><RefreshCw size={16} />{t('restart')}</Button><Button variant="danger" disabled={!serviceAvailable || serviceMutation.isPending} onClick={() => { setServiceConfirm('stop'); setServiceConfirmOpen(true) }}><Power size={16} />{t('stop')}</Button></div></div>
    </Card>
    {serviceConfirm ? <ConfirmDialog open={serviceConfirmOpen} title={serviceConfirm === 'stop' ? t('serviceStopTitle') : t('restart')} detail={serviceConfirm === 'stop' ? t('serviceStopWarning') : t('serviceRestartWarning')} acknowledgement={serviceConfirm === 'stop' ? t('serviceStopAcknowledgement') : undefined} confirmLabel={serviceConfirm === 'stop' ? t('stop') : t('restart')} cancelLabel={t('cancel')} pending={serviceMutation.isPending} onCancel={() => setServiceConfirmOpen(false)} onConfirm={() => serviceMutation.mutate(serviceConfirm)} afterOpenChange={(open) => { if (!open) setServiceConfirm(null) }} /> : null}
  </div>
}

function Version({ label, value }: { label: string; value: string }) {
  return <div><p className="text-xs text-[var(--muted)]">{label}</p><p className="mt-1 font-mono text-sm font-semibold">{value}</p></div>
}
