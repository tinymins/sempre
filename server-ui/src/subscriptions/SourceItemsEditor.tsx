import { AutoComplete, Button, Checkbox, Input, InputNumber, Select } from '@acme/components'
import { ArrowDown, ArrowUp, Play, Plus, Trash2 } from 'lucide-react'
import type { SubscriptionSource } from './types'
import { useI18n } from '../i18n/provider'

interface Props {
  value: SubscriptionSource[]
  onChange: (value: SubscriptionSource[]) => void
  onDebug: (source: SubscriptionSource, index: number) => void
}

const uaPresets = [
  { value: 'clash.meta', label: 'Clash Meta (default)' },
  { value: 'ClashforWindows/0.20.39', label: 'Clash for Windows' },
  { value: 'clash-verge/v2.2.3', label: 'Clash Verge' },
  { value: 'stash/2.7.6', label: 'Stash' },
  { value: 'Quantumult%20X/1.4.1', label: 'Quantumult X' },
  { value: 'Shadowrocket/2050', label: 'Shadowrocket' },
  { value: 'v2rayN/7.6', label: 'v2rayN' },
]

export function SourceItemsEditor({ value, onChange, onDebug }: Props) {
  const { t, number } = useI18n()
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
        <div key={index} className="space-y-2 rounded-lg border border-[var(--border)] p-3">
          <div className="flex items-center justify-between gap-2">
            <Checkbox checked={item.enabled} onChange={(event) => update(index, { enabled: event.target.checked })}>{t('source.enabled', { index: number(index + 1) })}</Checkbox>
            <div className="flex gap-1">
              <Button size="small" icon={<ArrowUp size={14} />} aria-label={t('source.moveUp', { index: number(index + 1) })} disabled={index === 0} onClick={() => move(index, -1)} />
              <Button size="small" icon={<ArrowDown size={14} />} aria-label={t('source.moveDown', { index: number(index + 1) })} disabled={index === value.length - 1} onClick={() => move(index, 1)} />
              <Button size="small" danger icon={<Trash2 size={14} />} aria-label={t('source.remove', { index: number(index + 1) })} onClick={() => onChange(value.filter((_, position) => position !== index))} />
              <Button size="small" icon={<Play size={14} />} aria-label={t('source.debug', { index: number(index + 1) })} disabled={!item.url.trim()} onClick={() => onDebug(item, index)} />
            </div>
          </div>
          <label className="block space-y-1 text-sm">{t('source.url')}
            <Input type="url" value={item.url} onChange={(event) => update(index, { url: event.target.value })} placeholder="https://example.com/subscription" />
          </label>
          <div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-5">
            <label className="block space-y-1 text-sm">{t('source.remark')}
              <Input value={item.remark} onChange={(event) => update(index, { remark: event.target.value })} />
            </label>
            <label className="block space-y-1 text-sm">{t('source.prefix')}
              <Input value={item.prefix} onChange={(event) => update(index, { prefix: event.target.value })} />
            </label>
            <label className="block space-y-1 text-sm">{t('source.cacheMinutes')}
              <InputNumber min={0} max={1440} value={item.cacheTtlMinutes} onChange={(next) => update(index, { cacheTtlMinutes: next ?? undefined })} className="w-full" />
            </label>
            <label className="block space-y-1 text-sm">{t('source.fetchMode')}
              <Select value={item.fetchMode ?? 'auto'} options={[{ value: 'auto', label: t('source.modeAuto') }, { value: 'domestic-direct', label: t('source.modeDirect') }]} onChange={(next) => update(index, { fetchMode: next as SubscriptionSource['fetchMode'] })} className="w-full" />
            </label>
            <label className="block space-y-1 text-sm">{t('source.fetchUa')}
              <AutoComplete value={item.fetchUa ?? ''} options={uaPresets} onChange={(next) => update(index, { fetchUa: next || undefined })} placeholder={t('source.uaPlaceholder')} className="w-full" />
            </label>
          </div>
        </div>
      ))}
      <Button icon={<Plus size={15} />} onClick={() => onChange([...value, { enabled: true, url: '', prefix: '', remark: '', fetchMode: 'auto' }])}>{t('source.add')}</Button>
    </div>
  )
}
