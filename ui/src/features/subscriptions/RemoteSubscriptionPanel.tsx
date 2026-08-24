import { ExternalLink, FileJson, LockKeyhole } from 'lucide-react'
import { useState } from 'react'
import { Modal } from '@acme/components'
import { Badge, Button, Card, Input, Spinner } from '../../components/ui'
import { api } from '../../lib/api'
import { useI18n } from '../../lib/i18n'
import { useSession } from '../../lib/session'
import type { SubscriptionProfile } from '../../lib/types'

export function RemoteSubscriptionPanel({ profile }: { profile: SubscriptionProfile }) {
  const { t } = useI18n()
  const { session } = useSession()
  const remote = profile.remote
  const [previewOpen, setPreviewOpen] = useState(false)
  const [previewLoading, setPreviewLoading] = useState(false)
  const [previewContent, setPreviewContent] = useState('')
  const [previewError, setPreviewError] = useState('')
  const preview = async () => {
    setPreviewOpen(true)
    setPreviewLoading(true)
    setPreviewError('')
    try {
      const result = await api<{ content: string }>(session!, `/subscriptions/${profile.id}/render`, {
        method: 'POST', body: JSON.stringify({ format: remote?.target || profile.last_compiler_target || 'sing-box-v13', force: false }),
      })
      setPreviewContent(result.content)
    } catch (reason) {
      setPreviewError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setPreviewLoading(false)
    }
  }
  return (
    <>
      <Card className="space-y-5 p-5">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <div className="flex items-center gap-2"><LockKeyhole size={17} /><h2 className="font-semibold">{t('remoteSubscriptionTitle')}</h2><Badge tone="info">{t('readOnly')}</Badge></div>
          <p className="mt-1 text-sm text-[var(--muted)]">{t('remoteSubscriptionDetail')}</p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button type="button" onClick={preview}><FileJson size={16} />{t('preview')}</Button>
          <Button type="button" disabled={!remote?.edit_url} onClick={() => remote?.edit_url && window.open(remote.edit_url, '_blank', 'noopener,noreferrer')}>
            <ExternalLink size={16} />{t('openServerEditor')}
          </Button>
        </div>
      </div>
      <label className="grid gap-1.5 text-sm font-medium">
        <span>{t('remoteManifestURL')}</span>
        <Input readOnly value={remote?.manifest_url ?? ''} />
      </label>
      <div className="grid gap-4 border-t border-[var(--border)] pt-4 text-sm sm:grid-cols-2 lg:grid-cols-3">
        <RemoteInfo label={t('serverProfile')} value={remote?.server_profile || '-'} />
        <RemoteInfo label={t('serverRevision')} value={remote?.server_revision ? String(remote.server_revision) : '-'} />
        <RemoteInfo label={t('compilerTarget')} value={remote?.target || profile.last_compiler_target || '-'} />
        <RemoteInfo label={t('nodeCount')} value={remote?.node_count === undefined ? '-' : String(remote.node_count)} />
        <RemoteInfo label={t('lastSynced')} value={formatTime(remote?.last_synced_at)} />
        <RemoteInfo label="SHA-256" value={remote?.artifact_sha256 || '-'} />
      </div>
      {profile.last_result ? <p className="border-t border-[var(--border)] pt-4 text-sm text-[var(--muted)]">{profile.last_result}</p> : null}
      </Card>
      <Modal open={previewOpen} title={t('preview')} footer={null} size="almost-full" onCancel={() => setPreviewOpen(false)} destroyOnClose>
        {previewLoading ? <div className="grid min-h-40 place-items-center"><Spinner /></div> : previewError ? <p role="alert" className="text-sm text-red-600 dark:text-red-400">{previewError}</p> : <pre className="max-h-[70vh] overflow-auto whitespace-pre-wrap break-words text-xs">{previewContent}</pre>}
      </Modal>
    </>
  )
}

function RemoteInfo({ label, value }: { label: string; value: string }) {
  return <div><p className="text-xs text-[var(--muted)]">{label}</p><p className="mt-1 break-all font-medium">{value}</p></div>
}

function formatTime(value?: string) {
  if (!value) return '-'
  const date = new Date(value)
  return Number.isNaN(date.valueOf()) ? value : date.toLocaleString()
}
