import { Button, Checkbox, Input, InputNumber, Select } from '@acme/components'
import { ArrowDown, ArrowUp, Play, Plus, Trash2 } from 'lucide-react'
import type { SubscriptionSource } from './types'

interface Props {
  value: SubscriptionSource[]
  onChange: (value: SubscriptionSource[]) => void
  onDebug: (source: SubscriptionSource) => void
}

export function SourceItemsEditor({ value, onChange, onDebug }: Props) {
  const update = (index: number, patch: Partial<SubscriptionSource>) => {
    onChange(value.map((item, position) => position === index ? { ...item, ...patch } : item))
  }
  const move = (index: number, direction: -1 | 1) => {
    const target = index + direction
    if (target < 0 || target >= value.length) return
    const next = [...value]
    ;[next[index], next[target]] = [next[target], next[index]]
    onChange(next)
  }

  return (
    <div className="space-y-3">
      {value.map((item, index) => (
        <div key={index} className="rounded-lg border border-[var(--border)] p-3 space-y-3">
          <div className="flex items-center justify-between gap-2">
            <Checkbox checked={item.enabled} onChange={(event) => update(index, { enabled: event.target.checked })}>启用来源 {index + 1}</Checkbox>
            <div className="flex gap-1">
              <Button size="small" icon={<ArrowUp size={14} />} aria-label={`上移来源 ${index + 1}`} disabled={index === 0} onClick={() => move(index, -1)} />
              <Button size="small" icon={<ArrowDown size={14} />} aria-label={`下移来源 ${index + 1}`} disabled={index === value.length - 1} onClick={() => move(index, 1)} />
              <Button size="small" danger icon={<Trash2 size={14} />} aria-label={`删除来源 ${index + 1}`} onClick={() => onChange(value.filter((_, position) => position !== index))} />
              <Button size="small" icon={<Play size={14} />} aria-label={`调试来源 ${index + 1}`} disabled={!item.url.trim()} onClick={() => onDebug(item)} />
            </div>
          </div>
          <label className="block space-y-1 text-sm">订阅地址
            <Input type="url" value={item.url} onChange={(event) => update(index, { url: event.target.value })} placeholder="https://example.com/subscription" />
          </label>
          <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            <label className="block space-y-1 text-sm">备注
              <Input value={item.remark} onChange={(event) => update(index, { remark: event.target.value })} />
            </label>
            <label className="block space-y-1 text-sm">节点名前缀
              <Input value={item.prefix} onChange={(event) => update(index, { prefix: event.target.value })} />
            </label>
            <label className="block space-y-1 text-sm">缓存分钟数
              <InputNumber min={0} max={1440} value={item.cacheTtlMinutes} onChange={(next) => update(index, { cacheTtlMinutes: next ?? undefined })} className="w-full" />
            </label>
            <label className="block space-y-1 text-sm">抓取模式
              <Select value={item.fetchMode ?? 'auto'} options={[{ value: 'auto', label: '自动' }, { value: 'domestic-direct', label: '国内直连' }]} onChange={(next) => update(index, { fetchMode: next as SubscriptionSource['fetchMode'] })} className="w-full" />
            </label>
            <label className="block space-y-1 text-sm sm:col-span-2">抓取 User-Agent
              <Input value={item.fetchUa ?? ''} onChange={(event) => update(index, { fetchUa: event.target.value || undefined })} placeholder="留空使用服务端默认值" />
            </label>
          </div>
        </div>
      ))}
      <Button icon={<Plus size={15} />} onClick={() => onChange([...value, { enabled: true, url: '', prefix: '', remark: '', fetchMode: 'auto' }])}>添加来源</Button>
    </div>
  )
}
