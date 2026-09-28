import { Button, CodePanel, Modal, Popconfirm } from '@acme/components'
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
      {manifestUrl ? <div className="mb-4 space-y-2"><p className="text-xs text-[var(--muted)]">{t('links.manifestHint')}</p><CodePanel title={t('links.manifest')} language="URL" maxHeight={140} bodyClassName="break-all whitespace-pre-wrap font-mono text-xs" actions={<><Button size="small" icon={<Copy size={14} />} aria-label={t('links.copyManifest')} onClick={() => void copy(manifestUrl)} /><Button size="small" icon={<ExternalLink size={14} />} aria-label={t('links.openManifest')} onClick={() => window.open(manifestUrl, '_blank', 'noopener,noreferrer')} /></>}>{manifestUrl}</CodePanel></div> : null}
      {subscription?.canDelete ? <Popconfirm title={t('links.clearTitle')} description={t('links.clearDetail')} okText={t('common.confirm')} cancelText={t('common.cancel')} onConfirm={() => void clearCache()}><Button size="small" loading={clearing} className="mb-4">{t('links.clear')}</Button></Popconfirm> : null}
      <h3 className="mb-2 text-sm font-semibold">{t('links.rawFormats')}</h3>
      <div className="space-y-2">
        {targets.map((target) => {
          const label = target.format
          const suffix = targetSuffix(target.format)
          if (!suffix) return null
          const url = `${base}/${suffix}`
          return <CodePanel key={target.format} title={label} language="URL" maxHeight={140} bodyClassName="break-all whitespace-pre-wrap font-mono text-xs" actions={<>
            <Button size="small" icon={<Copy size={14} />} aria-label={t('links.copyFormat', { format: label })} onClick={() => void copy(url)} />
            <Button size="small" icon={<ExternalLink size={14} />} aria-label={t('links.openFormat', { format: label })} onClick={() => window.open(url, '_blank', 'noopener,noreferrer')} />
            <Button size="small" icon={<Bug size={14} />} aria-label={t('links.debugFormat', { format: label })} title={t('links.debugFormat', { format: label })} onClick={() => setDebugTarget(target)} />
          </>}>{url}</CodePanel>
        })}
        {targets.length === 0 ? <p className="text-sm text-[var(--muted)]">{t('links.loadingFormats')}</p> : null}
      </div>
      {subscription && debugTarget ? <SubscriptionDebug savedSubscription={subscription} targets={targets} initialTarget={debugTarget} onClose={() => setDebugTarget(null)} /> : null}
    </Modal>
  )
}
