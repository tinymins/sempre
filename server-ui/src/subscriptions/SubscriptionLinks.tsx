import { useModalState, type ModalLifecycleProps } from '../use-modal-state'
import { Button, Modal, Popconfirm, Select, Tag, useToast } from '@acme/components'
import { Bug, Copy, ExternalLink, Globe2, Link2 } from 'lucide-react'
import { useState } from 'react'
import { targetSuffix, type Target } from './diagnostic-types'
import type { Subscription } from './types'
import { subscriptionApi } from './api'
import { SubscriptionDebug } from './SubscriptionDebug'
import { useI18n } from '../i18n/provider'

function formatLabel(format: string): string {
  const names: Record<string, string> = { clash: 'Clash', 'clash-meta': 'Clash Meta', 'clash-rs': 'Clash RS', xray: 'Xray', v2ray: 'V2Ray', dae: 'Dae' }
  if (names[format]) return names[format]
  const singBox = /^sing-box(?:-v(12|13|14))?(?:-(openwrt|windows|macos))?$/.exec(format)
  if (singBox) return `Sing-box v1.${singBox[1] ?? '11'}${singBox[2] === 'windows' ? ' Windows' : singBox[2] === 'macos' ? ' macOS' : ' OpenWrt'}`
  return format
}

function formatFamily(target: Target): string {
  if (target.format.startsWith('clash')) return 'clash'
  if (target.format.startsWith('sing-box')) return 'sing-box'
  return target.format
}

function targetPlatform(target: Target): string {
  return targetSuffix(target.format)?.split('/')[2] ?? target.platform
}

function LinkRow({ label, url, copyLabel, openLabel, debugLabel, onCopy, onDebug }: { label: string; url: string; copyLabel: string; openLabel: string; debugLabel?: string; onCopy: () => void; onDebug?: () => void }) {
  const { t } = useI18n()
  return <div className="rounded-lg border border-[var(--border)] p-3 transition-colors hover:bg-[var(--surface)]">
    <div className="flex flex-wrap items-center justify-between gap-2">
      <Tag color={label.toLowerCase().startsWith('clash') ? 'blue' : 'green'}><span className="inline-flex items-center gap-1">{onDebug && label.toLowerCase().startsWith('clash') ? <Globe2 size={13} /> : <Link2 size={13} />}{label}</span></Tag>
      <div className="flex flex-wrap items-center gap-1">
        <Button variant="text" size="small" className="text-blue-600 dark:text-blue-400" icon={<Copy size={14} />} aria-label={copyLabel} onClick={onCopy}>{t('common.copy')}</Button>
        <Button variant="text" size="small" className="text-green-600 dark:text-green-400" icon={<ExternalLink size={14} />} aria-label={openLabel} onClick={() => window.open(url, '_blank', 'noopener,noreferrer')}>{t('common.open')}</Button>
        {onDebug ? <Button variant="text" size="small" className="text-orange-600 dark:text-orange-400" icon={<Bug size={14} />} aria-label={debugLabel} onClick={onDebug}>{t('debug.run')}</Button> : null}
      </div>
    </div>
    <p className="mt-2 select-all break-all text-xs leading-5 text-[var(--muted)]">{url}</p>
  </div>
}

export function SubscriptionLinks({ open, afterOpenChange, subscription, targets, onClose }: { subscription: Subscription | null; targets: Target[]; onClose: () => void } & ModalLifecycleProps) {
  const { t, number } = useI18n()
  const toast = useToast()
  const [clearing, setClearing] = useState(false)
  const [formatFilter, setFormatFilter] = useState('')
  const [platformFilter, setPlatformFilter] = useState('')
  const formats = [...new Set(targets.map(formatFamily))]
  const platforms = [...new Set(targets.map(targetPlatform))]
  const platformNames: Record<string, string> = { windows: 'Windows', macos: 'macOS', openwrt: 'OpenWrt', default: t('links.genericPlatform') }
  const visibleTargets = targets.filter(target => (!formatFilter || formatFamily(target) === formatFilter) && (!platformFilter || targetPlatform(target) === platformFilter))
  const { value: debugTarget, show: setDebugTarget, close: closeDebug, ...debugModal } = useModalState<Target>()
  const base = subscription ? `${window.location.origin}/api/subscriptions/${encodeURIComponent(subscription.url)}` : ''
  const manifestUrl = subscription ? `${base}/manifest` : ''
  const copy = async (value: string) => {
    try {
      await navigator.clipboard.writeText(value)
      toast.success(t('common.copied'))
    } catch {
      toast.error(t('common.copyFailed'))
    }
  }
  const clearCache = async () => {
    if (!subscription) return
    setClearing(true)
    try {
      const result = await subscriptionApi.clearCache(subscription.id)
      toast.success(t('links.cleared', { count: number(result.cleared) }))
    } catch (reason) { toast.error(reason instanceof Error ? reason.message : String(reason)) }
    finally { setClearing(false) }
  }
  return (
    <Modal open={open} afterOpenChange={afterOpenChange} title={t('links.title', { name: subscription?.remark || t('configs.unnamed') })} footer={null} onCancel={onClose} width={560}>
      <p className="mb-4 text-sm text-[var(--muted)]">{t('links.stableHint')}</p>
      {manifestUrl ? <section className="mb-5 space-y-2"><p className="text-xs text-[var(--muted)]">{t('links.manifestHint')}</p><LinkRow label={t('links.manifest')} url={manifestUrl} copyLabel={t('links.copyManifest')} openLabel={t('links.openManifest')} onCopy={() => void copy(manifestUrl)} /></section> : null}
      <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
        <h3 className="text-sm font-semibold">{t('links.rawFormats')}</h3>
        <div className="ml-auto flex items-center gap-2">
          <Select size="small" aria-label={t('links.filterFormat')} value={formatFilter} onChange={(value: string) => setFormatFilter(value)} options={[{ value: '', label: t('links.allFormats') }, ...formats.map(value => ({ value, label: value === 'sing-box' ? 'Sing-box' : formatLabel(value) }))]} className="w-28" />
          <Select size="small" aria-label={t('links.filterPlatform')} value={platformFilter} onChange={(value: string) => setPlatformFilter(value)} options={[{ value: '', label: t('links.allPlatforms') }, ...platforms.map(value => ({ value, label: platformNames[value] ?? value }))]} className="w-28" />
        </div>
      </div>
      <div className="space-y-3">
        {visibleTargets.map((target) => {
          const label = formatLabel(target.format)
          const suffix = targetSuffix(target.format)
          if (!suffix) return null
          const url = `${base}/${suffix}`
          return <LinkRow key={target.format} label={label} url={url} copyLabel={t('links.copyFormat', { format: label })} openLabel={t('links.openFormat', { format: label })} debugLabel={t('links.debugFormat', { format: label })} onCopy={() => void copy(url)} onDebug={() => setDebugTarget(target)} />
        })}
        {targets.length === 0 ? <p className="text-sm text-[var(--muted)]">{t('links.loadingFormats')}</p> : null}
        {targets.length > 0 && visibleTargets.length === 0 ? <p className="text-sm text-[var(--muted)]">{t('links.noMatchingFormats')}</p> : null}
      </div>
      {subscription?.canDelete ? <div className="mt-5 border-t border-[var(--border)] pt-4"><Popconfirm title={t('links.clearTitle')} description={t('links.clearDetail')} okText={t('common.confirm')} cancelText={t('common.cancel')} onConfirm={() => void clearCache()}><Button size="small" loading={clearing}>{t('links.clear')}</Button></Popconfirm></div> : null}
      {subscription && debugTarget ? <SubscriptionDebug {...debugModal} savedSubscription={subscription} targets={targets} initialTarget={debugTarget} onClose={closeDebug} /> : null}
    </Modal>
  )
}
