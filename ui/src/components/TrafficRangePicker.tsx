import { useState } from 'react'
import { Button, Dropdown, Input } from '@acme/components'
import { Check, ChevronDown } from 'lucide-react'
import { useI18n } from '../lib/i18n'
import type { TrafficDimension } from '../lib/traffic'

type PresetRange = '1h' | '6h' | '12h' | '24h' | '7d' | '30d'

export type TrafficRange =
  | { key: 'period' | PresetRange }
  | { key: 'custom'; since: number; until: number }

const PRESET_HOURS: Record<PresetRange, number> = {
  '1h': 1,
  '6h': 6,
  '12h': 12,
  '24h': 24,
  '7d': 24 * 7,
  '30d': 24 * 30,
}

export function trafficHistoryPath(dimension: TrafficDimension, range: TrafficRange, now = Date.now()) {
  const params = new URLSearchParams({ dimension })
  if (range.key === 'custom') {
    params.set('since', String(range.since))
    params.set('until', String(range.until))
  } else if (range.key !== 'period') {
    params.set('since', String(now - PRESET_HOURS[range.key] * 60 * 60 * 1000))
  }
  return `/runtime/traffic/history?${params}`
}

export function TrafficRangePicker({ range, onChange }: { range: TrafficRange; onChange: (range: TrafficRange) => void }) {
  const { t } = useI18n()
  const [open, setOpen] = useState(false)
  const [customOpen, setCustomOpen] = useState(false)
  const [start, setStart] = useState('')
  const [end, setEnd] = useState('')
  const [error, setError] = useState(false)
  const options: Array<{ key: 'period' | PresetRange; label: string }> = [
    { key: 'period', label: t('rotationPolicy') },
    { key: '1h', label: t('lastHour') },
    { key: '6h', label: t('recent6Hours') },
    { key: '12h', label: t('recent12Hours') },
    { key: '24h', label: t('recent24Hours') },
    { key: '7d', label: t('recent7Days') },
    { key: '30d', label: t('recent30Days') },
  ]
  const currentLabel = range.key === 'custom'
    ? t('customRange')
    : options.find((option) => option.key === range.key)?.label

  const choosePreset = (key: 'period' | PresetRange) => {
    onChange({ key })
    setOpen(false)
  }
  const showCustom = () => {
    const until = range.key === 'custom' ? range.until : Date.now()
    const since = range.key === 'custom' ? range.since : until - 24 * 60 * 60 * 1000
    setStart(toLocalDateTime(since))
    setEnd(toLocalDateTime(until))
    setError(false)
    setCustomOpen(true)
  }
  const applyCustom = () => {
    const since = new Date(start).getTime()
    const until = new Date(end).getTime()
    if (!Number.isFinite(since) || !Number.isFinite(until) || since >= until) {
      setError(true)
      return
    }
    onChange({ key: 'custom', since, until })
    setOpen(false)
  }

  return <Dropdown
    trigger={['click']}
    placement="bottomRight"
    open={open}
    onOpenChange={(next) => {
      setOpen(next)
      if (!next) setCustomOpen(false)
    }}
    dropdownRender={() => <div className="w-72 p-1.5">
      {options.map((option) => <Button key={option.key} variant="text" block className={`!justify-start ${range.key === option.key ? 'bg-black/[0.04] dark:bg-white/[0.06]' : ''}`} onClick={() => choosePreset(option.key)}>
        <Check className={range.key === option.key ? '' : 'invisible'} size={14} />
        {option.label}
      </Button>)}
      <div className="my-1 h-px bg-black/[0.06] dark:bg-white/[0.08]" />
      <Button variant="text" block className={`!justify-start ${range.key === 'custom' ? 'bg-black/[0.04] dark:bg-white/[0.06]' : ''}`} onClick={showCustom}>
        <Check className={range.key === 'custom' ? '' : 'invisible'} size={14} />
        {t('customRange')}
      </Button>
      {customOpen ? <div className="mt-1 grid gap-3 border-t border-[var(--border)] p-2 pt-3">
        <label className="grid gap-1 text-xs font-medium"><span>{t('startTime')}</span><Input type="datetime-local" value={start} onChange={(event) => setStart(event.target.value)} /></label>
        <label className="grid gap-1 text-xs font-medium"><span>{t('endTime')}</span><Input type="datetime-local" value={end} onChange={(event) => setEnd(event.target.value)} /></label>
        {error ? <p className="text-xs text-red-500" role="alert">{t('invalidTimeRange')}</p> : null}
        <Button variant="primary" block onClick={applyCustom}>{t('apply')}</Button>
      </div> : null}
    </div>}
  >
    <Button className="min-w-36 justify-between" aria-label={currentLabel}>
      <span>{currentLabel}</span><ChevronDown size={14} />
    </Button>
  </Dropdown>
}

function toLocalDateTime(timestamp: number) {
  const date = new Date(timestamp)
  const part = (value: number) => String(value).padStart(2, '0')
  return `${date.getFullYear()}-${part(date.getMonth() + 1)}-${part(date.getDate())}T${part(date.getHours())}:${part(date.getMinutes())}`
}
