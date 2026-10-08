import { AutoComplete, Button, Checkbox, Input, InputNumber, Select, Tag, TextArea, Tooltip } from '@acme/components'
import { Play, Trash2 } from 'lucide-react'
import type { ReactNode } from 'react'
import { useEditorI18n } from './i18n'
import type { EditorSource } from './sources'

interface Props {
  source: EditorSource
  index: number
  readOnly?: boolean
  handle: ReactNode
  uaPresets: { value: string; label: string }[]
  update: (patch: Partial<EditorSource>) => void
  remove: () => void
  debug?: () => void
}

export function PageSourceFields({ source, index, readOnly, handle, uaPresets, update, remove, debug }: Props) {
  const { t, number } = useEditorI18n()
  const remark = <Input size="small" value={source.remark} disabled={readOnly} aria-label={`${t('source.remark')} · ${number(index + 1)}`} placeholder={t('source.remark')} onChange={event => update({ remark: event.target.value })} className="min-w-0 md:flex-1" />
  const deleteButton = <Button disabled={readOnly} variant="text" size="small" danger aria-label={t('source.remove', { index: number(index + 1) })} icon={source.type === 'url' ? <Trash2 size={14} /> : undefined} onClick={remove} className="shrink-0">{source.type === 'raw' ? t('common.delete') : null}</Button>
  if (source.type === 'raw') return <>
    <div className="mb-2 flex items-center gap-2"><Tag>RAW</Tag>{remark}{deleteButton}</div>
    <TextArea rows={8} readOnly={readOnly} value={source.content} aria-label="RAW" placeholder="proxies:" onChange={event => update({ content: event.target.value })} className="font-mono" />
  </>
  return <div className="flex gap-2">
    <div className="flex shrink-0 items-center gap-2 self-start pt-[5px]">{handle}<Tooltip title={t(source.enabled ? 'source.enabledHelp' : 'source.disabledHelp')}><Checkbox disabled={readOnly} checked={source.enabled} aria-label={t('source.enabled', { index: number(index + 1) })} onChange={event => update({ enabled: event.target.checked })} /></Tooltip></div>
    <div className="min-w-0 flex-1 space-y-2">
      <div className="flex items-center gap-2"><Input size="small" value={source.url} disabled={readOnly} aria-label={t('source.url')} placeholder={t('source.url')} status={source.enabled && !source.url.trim() ? 'warning' : undefined} onChange={event => update({ url: event.target.value })} className="min-w-0 flex-1" />{deleteButton}</div>
      <div className="flex flex-col gap-2 md:flex-row md:items-center">
        {remark}
        <div className="flex shrink-0 items-center gap-2">
          <div className="flex-1 md:w-[120px] md:flex-none"><Input size="small" value={source.prefix} disabled={readOnly} aria-label={t('source.prefix')} placeholder={t('source.prefix')} onChange={event => update({ prefix: event.target.value })} suffix={<Tooltip title={t('source.prefixHelp')}><span className="cursor-help text-xs text-gray-400">?</span></Tooltip>} /></div>
          <Tooltip title={t('source.cacheHelp')}><InputNumber size="small" min={0} max={1440} value={source.cacheTtlMinutes} disabled={readOnly} aria-label={t('source.cacheMinutes')} placeholder={t('source.cachePlaceholder')} onChange={next => update({ cacheTtlMinutes: next ?? undefined })} addonAfter={t('source.cacheUnit')} style={{ width: 130 }} /></Tooltip>
        </div>
        <div className="flex items-center gap-2 md:flex-1">
          <Tooltip title={t('source.modeHelp')}><div className="w-[120px] shrink-0"><Select size="small" value={source.fetchMode ?? 'auto'} disabled={readOnly} options={[{ value: 'auto', label: t('source.modeAuto') }, { value: 'domestic-direct', label: t('source.modeDirect') }]} onChange={next => update({ fetchMode: next as EditorSource['fetchMode'] })} className="w-full" /></div></Tooltip>
          <Tooltip title={t('source.uaHelp')}><div className="flex-1"><AutoComplete size="small" value={source.fetchUa ?? ''} disabled={readOnly} options={uaPresets} onChange={next => update({ fetchUa: next || undefined })} placeholder={t('source.uaPlaceholder')} filterOption={(input, option) => option.value.toLowerCase().includes(input.toLowerCase()) || (typeof option.label === 'string' && option.label.toLowerCase().includes(input.toLowerCase()))} allowClear className="w-full" /></div></Tooltip>
          {debug ? <Tooltip title={t('source.debugHelp')}><Button variant="text" size="small" icon={<Play size={14} />} aria-label={t('source.debug', { index: number(index + 1) })} disabled={!source.url.trim()} onClick={debug} className="shrink-0" /></Tooltip> : null}
        </div>
      </div>
    </div>
  </div>
}
