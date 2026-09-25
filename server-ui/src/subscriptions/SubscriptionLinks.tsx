import { Button, Modal, Popconfirm } from '@acme/components'
import { Copy, ExternalLink } from 'lucide-react'
import { useState } from 'react'
import { targetSuffix, type Target } from './diagnostic-types'
import type { Subscription } from './types'
import { subscriptionApi } from './api'

export function SubscriptionLinks({ subscription, targets, onClose }: { subscription: Subscription | null; targets: Target[]; onClose: () => void }) {
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [clearing, setClearing] = useState(false)
  const base = subscription ? `${window.location.origin}/api/public/proxy/${encodeURIComponent(subscription.url)}` : ''
  const manifestUrl = subscription ? `${window.location.origin}/api/v1/public/subscriptions/${encodeURIComponent(subscription.url)}` : ''
  const copy = async (value: string) => {
    try {
      await navigator.clipboard.writeText(value)
      setError('')
    } catch {
      setError('复制失败，请检查浏览器剪贴板权限。')
    }
  }
  const clearCache = async () => {
    if (!subscription) return
    setClearing(true); setError(''); setNotice('')
    try {
      const result = await subscriptionApi.clearCache(subscription.id)
      setNotice(`已清除此订阅的 ${result.cleared} 项来源及规则缓存。`)
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)) }
    finally { setClearing(false) }
  }
  return (
    <Modal open={Boolean(subscription)} title={`订阅链接 · ${subscription?.remark || '未命名配置集'}`} footer={null} onCancel={onClose} size="large">
      {error ? <p role="alert" className="mb-3 text-sm text-red-600">{error}</p> : null}
      {notice ? <p role="status" className="mb-3 text-sm text-emerald-600">{notice}</p> : null}
      <p className="mb-4 text-sm text-[var(--muted)]">链接使用订阅的稳定标识，保存配置后无需重新生成。</p>
      {manifestUrl ? <div className="mb-4 space-y-2 rounded-lg border border-[var(--border)] p-3"><strong className="text-sm">Sempre 客户端订阅</strong><p className="text-xs text-[var(--muted)]">在 Sempre 客户端添加远程订阅时使用此入口；客户端会按所选目标格式请求配置。</p><div className="flex items-center gap-2"><code className="min-w-0 flex-1 truncate text-xs" title={manifestUrl}>{manifestUrl}</code><Button size="small" icon={<Copy size={14} />} aria-label="复制 Sempre 客户端订阅链接" onClick={() => void copy(manifestUrl)} /><Button size="small" icon={<ExternalLink size={14} />} aria-label="打开 Sempre 客户端订阅链接" onClick={() => window.open(manifestUrl, '_blank', 'noopener,noreferrer')} /></div></div> : null}
      {subscription?.canDelete ? <Popconfirm title="清除此订阅缓存？" description="下次生成时将重新获取订阅来源和规则。" onConfirm={() => void clearCache()}><Button size="small" loading={clearing} className="mb-4">清除此订阅缓存</Button></Popconfirm> : null}
      <h3 className="mb-2 text-sm font-semibold">目标格式原始配置</h3>
      <div className="space-y-2">
        {targets.map((target) => {
          const label = target.format
          const suffix = targetSuffix(target.format)
          if (!suffix) return null
          const url = `${base}/${suffix}`
          return <div key={target.format} className="flex items-center gap-2 rounded-lg border border-[var(--border)] p-2">
            <span className="w-40 shrink-0 text-sm">{label}</span>
            <code className="min-w-0 flex-1 truncate text-xs" title={url}>{url}</code>
            <Button size="small" icon={<Copy size={14} />} aria-label={`复制 ${label} 链接`} onClick={() => void copy(url)} />
            <Button size="small" icon={<ExternalLink size={14} />} aria-label={`打开 ${label} 链接`} onClick={() => window.open(url, '_blank', 'noopener,noreferrer')} />
          </div>
        })}
        {targets.length === 0 ? <p className="text-sm text-[var(--muted)]">正在加载可用输出格式…</p> : null}
      </div>
    </Modal>
  )
}
