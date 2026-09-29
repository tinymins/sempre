import { DndContext, KeyboardSensor, PointerSensor, closestCenter, useSensor, useSensors } from '@dnd-kit/core'
import { SortableContext, sortableKeyboardCoordinates, verticalListSortingStrategy } from '@dnd-kit/sortable'
import { SortableSource } from './SortableSource'
import { AutoComplete, Button, Checkbox, CodeEditor, Input, InputNumber, Select } from '@acme/components'
import { ArrowDown, ArrowUp, Play, Plus, Trash2 } from 'lucide-react'
import { createSource, moveSource, sourceText, type EditorSource as SubscriptionSource } from './sources'
import { useEditorI18n as useI18n } from './i18n'

interface Props {
  readOnly?: boolean
  value?: SubscriptionSource[]
  onChange?: (value: SubscriptionSource[]) => void
  onDebug?: (source: SubscriptionSource, index: number) => void
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

export function SourceListEditor({ readOnly, value = [], onChange, onDebug }: Props) {
  const { t, number } = useI18n()
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 6 } }), useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }))
  const update = (index: number, patch: Partial<SubscriptionSource>) => {
    if (readOnly) return
    onChange?.(value.map((item, position) => position === index ? { ...item, ...patch } as SubscriptionSource : item))
  }
  const move = (index: number, direction: -1 | 1) => {
    const target = index + direction
    if (readOnly || target < 0 || target >= value.length) return
    onChange?.(moveSource(value, index, target))
  }

  return (
    <div className="space-y-3">
      <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={({ active, over }) => {
        if (!readOnly && over && active.id !== over.id) onChange?.(moveSource(value, value.findIndex(item => item.id === active.id), value.findIndex(item => item.id === over.id)))
      }}><SortableContext items={value.map(item => item.id)} strategy={verticalListSortingStrategy}>
      {value.map((item, index) => (
        <SortableSource key={item.id} id={item.id} index={index} disabled={readOnly}>{handle => <>
          <div className="flex items-center justify-between gap-2">
            <Checkbox disabled={readOnly} checked={item.enabled} onChange={(event) => update(index, { enabled: event.target.checked })}>{t('source.enabled', { index: number(index + 1) })}</Checkbox>
            <div className="flex gap-1">{handle}
              <Button size="small" icon={<ArrowUp size={14} />} aria-label={t('source.moveUp', { index: number(index + 1) })} disabled={readOnly || index === 0} onClick={() => move(index, -1)} />
              <Button size="small" icon={<ArrowDown size={14} />} aria-label={t('source.moveDown', { index: number(index + 1) })} disabled={readOnly || index === value.length - 1} onClick={() => move(index, 1)} />
              <Button disabled={readOnly} size="small" danger icon={<Trash2 size={14} />} aria-label={t('source.remove', { index: number(index + 1) })} onClick={() => onChange?.(value.filter((_, position) => position !== index))} />
              {onDebug && item.type === 'url' ? <Button size="small" icon={<Play size={14} />} aria-label={t('source.debug', { index: number(index + 1) })} disabled={!sourceText(item).trim()} onClick={() => onDebug(item, index)} /> : null}
            </div>
          </div>
          {item.type === 'raw' ? <CodeEditor language="plaintext" readOnly={readOnly} ariaLabel="RAW" value={item.content} onChange={content => update(index, { content })} height={200} /> : <label className="block space-y-1 text-sm">{t('source.url')}
            <Input type="url" value={item.url} onChange={(event) => update(index, { url: event.target.value })} placeholder="https://example.com/subscription" />
          </label>}
          <div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-5">
            <label className="block space-y-1 text-sm">{t('source.remark')}
              <Input aria-label={`${t("source.remark")} · ${number(index + 1)}`} value={item.remark} onChange={(event) => update(index, { remark: event.target.value })} />
            </label>
            <label className="block space-y-1 text-sm">{t('source.prefix')}
              <Input value={item.prefix} onChange={(event) => update(index, { prefix: event.target.value })} />
            </label>
            {item.type === 'url' ? <><label className="block space-y-1 text-sm">{t('source.cacheMinutes')}
              <InputNumber min={0} max={1440} value={item.cacheTtlMinutes} onChange={(next) => update(index, { cacheTtlMinutes: next ?? undefined })} className="w-full" />
            </label>
            <label className="block space-y-1 text-sm">{t('source.fetchMode')}
              <Select value={item.fetchMode ?? 'auto'} options={[{ value: 'auto', label: t('source.modeAuto') }, { value: 'domestic-direct', label: t('source.modeDirect') }]} onChange={(next) => update(index, { fetchMode: next as SubscriptionSource['fetchMode'] })} className="w-full" />
            </label>
            <label className="block space-y-1 text-sm">{t('source.fetchUa')}
              <AutoComplete value={item.fetchUa ?? ''} options={uaPresets} onChange={(next) => update(index, { fetchUa: next || undefined })} placeholder={t('source.uaPlaceholder')} className="w-full" />
            </label></> : null}
          </div>
        </>}</SortableSource>
      ))}
      </SortableContext></DndContext>
      <Button disabled={readOnly} icon={<Plus size={15} />} onClick={() => onChange?.([...value, createSource()])}>{t('source.add')}</Button>
      <Button disabled={readOnly} className="ml-2" icon={<Plus size={15} />} onClick={() => onChange?.([...value, createSource('raw')])}>{t('source.addRaw')}</Button>
    </div>
  )
}
