import { Button, Modal, Popconfirm } from '@acme/components'
import { Bug, Copy, ExternalLink } from 'lucide-react'
import { useState } from 'react'
import { targetSuffix, type Target } from './diagnostic-types'
import type { Subscription } from './types'
import { subscriptionApi } from './api'
import { SubscriptionDebug } from './SubscriptionDebug'
import { useI18n } from '../i18n/provider'

export function SubscriptionLinks({ subscription, targets, onClose }: { subscription: Subscription | null; targets: Target[]; onClose: () => void }) {
  const { t, number } = useI18n()
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [clearing, setClearing] = useState(false)
  const [debugTarget, setDebugTarget] = useState<Target | null>(null)
  const base = subscription ? `${window.location.origin}/api/public/proxy/${encodeURIComponent(subscription.url)}` : ''
  const manifestUrl = subscription ? `${window.location.origin}/api/v1/public/subscriptions/${encodeURIComponent(subscription.url)}` : ''
  const copy = async (value: string) => {
    try {
      await navigator.clipboard.writeText(value)
      setError('')
    } catch {
      setError(t('common.copyFailed'))
    }
  }
  const clearCache = async () => {
    if (!subscription) return
    setClearing(true); setError(''); setNotice('')
    try {
      const result = await subscriptionApi.clearCache(subscription.id)
      setNotice(t('links.cleared', { count: number(result.cleared) }))
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)) }
    finally { setClearing(false) }
  }
  return (
    <Modal open={Boolean(subscription)} title={t('links.title', { name: subscription?.remark || t('configs.unnamed') })} footer={null} onCancel={onClose} size="large">
      {error ? <p role="alert" className="mb-3 text-sm text-red-600">{error}</p> : null}
      {notice ? <p role="status" className="mb-3 text-sm text-emerald-600">{notice}</p> : null}
      <p className="mb-4 text-sm text-[var(--muted)]">{t('links.stableHint')}</p>
      {manifestUrl ? <div className="mb-4 space-y-2 rounded-lg border border-[var(--border)] p-3"><strong className="text-sm">{t('links.manifest')}</strong><p className="text-xs text-[var(--muted)]">{t('links.manifestHint')}</p><div className="flex items-center gap-2"><code className="min-w-0 flex-1 truncate text-xs" title={manifestUrl}>{manifestUrl}</code><Button size="small" icon={<Copy size={14} />} aria-label={t('links.copyManifest')} onClick={() => void copy(manifestUrl)} /><Button size="small" icon={<ExternalLink size={14} />} aria-label={t('links.openManifest')} onClick={() => window.open(manifestUrl, '_blank', 'noopener,noreferrer')} /></div></div> : null}
      {subscription?.canDelete ? <Popconfirm title={t('links.clearTitle')} description={t('links.clearDetail')} okText={t('common.confirm')} cancelText={t('common.cancel')} onConfirm={() => void clearCache()}><Button size="small" loading={clearing} className="mb-4">{t('links.clear')}</Button></Popconfirm> : null}
      <h3 className="mb-2 text-sm font-semibold">{t('links.rawFormats')}</h3>
      <div className="space-y-2">
        {targets.map((target) => {
          const label = target.format
          const suffix = targetSuffix(target.format)
          if (!suffix) return null
          const url = `${base}/${suffix}`
          return <div key={target.format} className="flex items-center gap-2 rounded-lg border border-[var(--border)] p-2">
            <span className="w-40 shrink-0 text-sm">{label}</span>
            <code className="min-w-0 flex-1 truncate text-xs" title={url}>{url}</code>
            <Button size="small" icon={<Copy size={14} />} aria-label={t('links.copyFormat', { format: label })} onClick={() => void copy(url)} />
            <Button size="small" icon={<ExternalLink size={14} />} aria-label={t('links.openFormat', { format: label })} onClick={() => window.open(url, '_blank', 'noopener,noreferrer')} />
            <Button size="small" icon={<Bug size={14} />} aria-label={t('links.debugFormat', { format: label })} title={t('links.debugFormat', { format: label })} onClick={() => setDebugTarget(target)} />
          </div>
        })}
        {targets.length === 0 ? <p className="text-sm text-[var(--muted)]">{t('links.loadingFormats')}</p> : null}
      </div>
      {subscription && debugTarget ? <SubscriptionDebug savedSubscription={subscription} targets={targets} initialTarget={debugTarget} onClose={() => setDebugTarget(null)} /> : null}
    </Modal>
  )
}
