import { useRef, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { CheckCircle2, Download, LoaderCircle, Power, RefreshCw, ServerCog, ShieldAlert, Upload } from 'lucide-react'
import { Switch } from '@acme/components'
import { api } from '../../lib/api'
import { useI18n } from '../../lib/i18n'
import { useSession } from '../../lib/session'
import type { ServiceUpdateStatus, SystemStatus } from '../../lib/types'
import { useServiceUpdateFlow } from './ServiceUpdateFlow'
import { Badge, Button, Card, ConfirmDialog, Spinner } from '../../components/ui'
import { ReleaseNotes } from './ReleaseNotes'

export function ServicePanel() {
  const { locale, t } = useI18n()
  const { session } = useSession()
  const queryClient = useQueryClient()
  const updatePackageInput = useRef<HTMLInputElement>(null)
  const [previewConfirmOpen, setPreviewConfirmOpen] = useState(false)
  const system = useQuery({ queryKey: ['system'], queryFn: () => api<SystemStatus>(session!, '/system') })
  const update = useQuery({ queryKey: ['service', 'update'], queryFn: () => api<ServiceUpdateStatus>(session!, '/service/update'), enabled: false, retry: false })
  const settings = useQuery({ queryKey: ['service', 'update-settings'], queryFn: () => api<ServiceUpdateSettingsResponse>(session!, '/service/update/settings'), retry: false })
  const saveSettings = useMutation({
    mutationFn: (allow_prerelease: boolean) => api<ServiceUpdateSettingsResponse>(session!, '/service/update/settings', { method: 'PUT', body: JSON.stringify({ allow_prerelease }) }),
    onSuccess: (result) => {
      queryClient.setQueryData(['service', 'update-settings'], result)
      queryClient.removeQueries({ queryKey: ['service', 'update'], exact: true })
      setPreviewConfirmOpen(false)
    },
  })
  const { task: updateTask, mutation: upgrade, uploadMutation, openProgress } = useServiceUpdateFlow()
  const serviceAvailable = system.data?.mode === 'system' && system.data.service !== 'not installed'
  const currentVersion = update.data?.current_version ?? system.data?.version ?? '-'
  const updating = upgrade.isPending || uploadMutation.isPending || updateTask?.state === 'running'
  const releaseHistory = (update.data?.release_history?.length ? update.data.release_history : update.data ? [{ version: update.data.latest_version, published_at: update.data.published_at, notes: update.data.release_notes }] : [])
    .map((release) => ({ ...release, notes: release.notes || t('noReleaseNotes') }))
  const zh = locale === 'zh-CN'
  const previewCopy = zh ? {
    label: '允许更新到测试版',
    detail: '检查并更新到版本号更高的预发布版本，例如 beta、dev 或 test。',
    title: '允许更新到测试版？',
    warning: '测试版可能不稳定。开启后，检查更新和一键升级会使用包含预发布版本的通道；草稿版本仍不会被包含。',
    confirm: '允许测试版更新',
  } : {
    label: 'Allow preview updates',
    detail: 'Check for and install newer prerelease versions such as beta, dev, or test.',
    title: 'Allow preview updates?',
    warning: 'Preview builds may be unstable. Update checks and one-click upgrades will use the prerelease channel; draft releases remain excluded.',
    confirm: 'Allow preview updates',
  }
  const uploadCopy = zh ? '上传更新包' : 'Upload update package'

  function uploadPackage(file?: File) {
    if (!file || updating) return
    openProgress()
    uploadMutation.mutate({ file })
  }

  return <Card className="p-4 md:p-5">
      <div className="mb-5 flex items-center gap-2"><ServerCog size={18} className="text-emerald-600" /><h2 className="text-sm font-semibold">{t('serviceUpdateTitle')}</h2></div>
      <div className="mb-4 flex flex-wrap items-center gap-2">
        <div className="inline-flex h-9 shrink-0 items-center gap-2 rounded-md border border-[var(--border)] bg-[var(--surface)] px-3 text-sm font-medium" title={previewCopy.detail}>
          <Switch size="small" aria-label={previewCopy.label} checked={settings.data?.settings.allow_prerelease ?? false} loading={settings.isFetching || saveSettings.isPending} onChange={(checked) => checked ? setPreviewConfirmOpen(true) : saveSettings.mutate(false)} />
          <span>{previewCopy.label}</span>
        </div>
        <Button disabled={!updating && update.isFetching} onClick={() => updating ? openProgress() : void update.refetch()}>{updating ? <LoaderCircle size={16} className="animate-spin" /> : update.isFetching ? <Spinner /> : <RefreshCw size={16} />}{updating ? t('serviceUpdateViewProgress') : t('checkForUpdates')}</Button>
        <Button disabled={!serviceAvailable || updating} onClick={() => updatePackageInput.current?.click()}><Upload size={16} />{uploadCopy}</Button>
        <input ref={updatePackageInput} aria-label={uploadCopy} className="sr-only" type="file" accept=".zip,.tar.gz,application/zip,application/gzip" disabled={!serviceAvailable || updating} onChange={(event) => { uploadPackage(event.target.files?.[0]); event.target.value = '' }} />
      </div>
      {settings.isError || saveSettings.isError ? <p role="alert" className="mb-4 text-sm text-red-600 dark:text-red-400">{(saveSettings.error || settings.error)?.message}</p> : null}
      <div className="grid gap-3 rounded-lg bg-[var(--surface-hover)] p-4 sm:grid-cols-2">
        <Version label={t('currentVersion')} value={currentVersion} />
        <Version label={t('latestVersion')} value={update.data?.latest_version ?? t('notChecked')} />
      </div>
      {update.isError ? <p role="alert" className="mt-4 text-sm text-red-600 dark:text-red-400">{update.error.message}</p> : null}
      {update.data ? <div className="mt-4 space-y-4">
        <div className="flex flex-wrap items-center gap-3"><Badge tone={update.data.update_available ? 'warning' : 'success'}>{update.data.update_available ? t('updateAvailable') : t('upToDate')}</Badge>{update.data.published_at ? <span className="text-xs text-[var(--muted)]">{new Date(update.data.published_at).toLocaleString()}</span> : null}</div>
        {update.data.update_available ? <><div><h3 className="mb-2 text-sm font-semibold">{t('releaseNotes')}</h3><ReleaseNotes releases={releaseHistory.length ? releaseHistory : [{ version: update.data.latest_version, published_at: update.data.published_at, notes: t('noReleaseNotes') }]} locale={locale} /></div><Button variant="primary" disabled={!serviceAvailable} onClick={() => { openProgress(); if (!updating) upgrade.mutate() }}>{updating ? <LoaderCircle size={16} className="animate-spin" /> : <Download size={16} />}{updating ? t('serviceUpdateViewProgress') : t('upgradeNow')}</Button>{!serviceAvailable ? <p className="text-xs text-[var(--muted)]">{t('systemServiceUpdateOnly')}</p> : null}</> : <div className="flex items-center gap-2 text-sm text-emerald-600 dark:text-emerald-400"><CheckCircle2 size={17} />{t('upToDateDetail')}</div>}
      </div> : null}
      <ConfirmDialog open={previewConfirmOpen} title={previewCopy.title} detail={previewCopy.warning} confirmLabel={previewCopy.confirm} cancelLabel={t('cancel')} pending={saveSettings.isPending} onCancel={() => setPreviewConfirmOpen(false)} onConfirm={() => saveSettings.mutate(true)} />
  </Card>
}

interface ServiceUpdateSettingsResponse {
  settings: { schema: number; allow_prerelease: boolean }
}

export function ServiceActionsPanel() {
  const { t } = useI18n()
  const { session } = useSession()
  const [notice, setNotice] = useState('')
  const [serviceConfirm, setServiceConfirm] = useState<'restart' | 'stop' | null>(null)
  const [serviceConfirmOpen, setServiceConfirmOpen] = useState(false)
  const system = useQuery({ queryKey: ['system'], queryFn: () => api<SystemStatus>(session!, '/system') })
  const serviceMutation = useMutation({
    mutationFn: (action: string) => api(session!, '/service/action', { method: 'POST', body: JSON.stringify({ action }) }),
    onSuccess: () => { setNotice(t('operationAccepted')); setServiceConfirmOpen(false) },
    onError: (error) => setNotice(error.message),
  })
  const serviceAvailable = system.data?.mode === 'system' && system.data.service !== 'not installed'

  return <div className="min-w-0 space-y-5">
    {notice ? <div role="status" className="border-l-2 border-emerald-500 bg-emerald-500/8 px-3 py-2 text-sm">{notice}</div> : null}
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
