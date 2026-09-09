import { useMemo, useState } from 'react'
import { Alert } from '@acme/components'
import { Plus, Save, Trash2 } from 'lucide-react'
import { Button, Card, Input, Spinner } from '../../components/ui'
import { useI18n } from '../../lib/i18n'
import type { SubscriptionProfile, SubscriptionSource } from '../../lib/types'

function emptySource(): SubscriptionSource {
  return { id: crypto.randomUUID(), type: 'url', enabled: true, url: '', fetch_mode: 'auto' }
}

function validURL(value: string) {
  try {
    const parsed = new URL(value)
    return ['http:', 'https:'].includes(parsed.protocol) && !parsed.username && !parsed.password
  } catch {
    return false
  }
}

export function SimpleSubscriptionEditor({ profile, saving, onSave }: { profile: SubscriptionProfile; saving: boolean; onSave: (profile: SubscriptionProfile) => Promise<void> }) {
  const { locale } = useI18n()
  const zh = locale === 'zh-CN'
  const initial = useMemo(() => profile.sources.filter((source) => source.type === 'url'), [profile])
  const [sources, setSources] = useState<SubscriptionSource[]>(initial.length ? initial : [emptySource()])
  const [error, setError] = useState('')
  const dirty = JSON.stringify(sources) !== JSON.stringify(initial)

  if (profile.mode === 'remote') {
    return <Alert type="info" showIcon message={zh ? '这个订阅由远程服务管理，请切换到本机模式（专业）查看详情。' : 'This subscription is managed remotely. Switch to Local mode (advanced) for details.'} />
  }

  const save = async () => {
    const normalized = sources.map((source) => ({ ...source, url: source.url?.trim() }))
    if (!normalized.length || normalized.some((source) => !source.url || !validURL(source.url))) {
      setError(zh ? '请输入有效的 HTTP 或 HTTPS 订阅 URL。' : 'Enter a valid HTTP or HTTPS subscription URL.')
      return
    }
    const byID = new Map(normalized.map((source) => [source.id, source]))
    const retained = new Set<string>()
    const ordered = profile.sources.flatMap((source) => {
      if (source.type !== 'url') return [source]
      const replacement = byID.get(source.id)
      if (!replacement) return []
      retained.add(source.id)
      return [replacement]
    })
    normalized.forEach((source) => {
      if (!retained.has(source.id)) ordered.push(source)
    })
    setError('')
    await onSave({ ...profile, sources: ordered })
  }

  return <Card className="p-4 md:p-5">
    <div className="mb-5"><h2 className="text-sm font-semibold">{zh ? '订阅 URL' : 'Subscription URLs'}</h2><p className="mt-1 text-xs text-[var(--muted)]">{zh ? '添加一个或多个订阅地址，保存时会保留专业模式中的其他配置。' : 'Add multiple URLs without changing other advanced settings.'}</p></div>
    <div className="space-y-3">
      {sources.map((source, index) => <div key={source.id} className="flex gap-2">
        <Input aria-label={`${zh ? '订阅 URL' : 'Subscription URL'} ${index + 1}`} value={source.url ?? ''} placeholder="https://example.com/subscription" onChange={(event) => setSources((current) => current.map((item) => item.id === source.id ? { ...item, url: event.target.value } : item))} />
        <Button size="icon" variant="ghost" aria-label={zh ? '删除订阅 URL' : 'Delete subscription URL'} disabled={sources.length === 1} onClick={() => setSources((current) => current.filter((item) => item.id !== source.id))}><Trash2 size={16} /></Button>
      </div>)}
    </div>
    <div className="mt-4 flex flex-wrap items-center gap-2">
      <Button onClick={() => setSources((current) => [...current, emptySource()])}><Plus size={16} />{zh ? '添加订阅 URL' : 'Add subscription URL'}</Button>
      <Button variant="primary" disabled={!dirty || saving} onClick={() => void save()}>{saving ? <Spinner /> : <Save size={16} />}{zh ? '保存' : 'Save'}</Button>
    </div>
    {error ? <p role="alert" className="mt-3 text-sm text-red-600">{error}</p> : null}
  </Card>
}
