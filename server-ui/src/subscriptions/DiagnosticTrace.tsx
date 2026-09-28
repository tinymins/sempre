import { Button, Input, Modal } from '@acme/components'
import { CheckCircle2, CircleMinus, CircleX } from 'lucide-react'
import { useState } from 'react'
import type { NodeTraceResult } from './diagnostic-types'
import { diagnosticText } from './diagnostic-locale'
import { useI18n } from '../i18n/provider'
import { DiagnosticValue, hasStructuredCode } from './DiagnosticValue'
import { ServerCodeBlock } from '../ServerCodeBlock'

function traceText(value: string): { text: string; language: string } {
  try {
    return { text: JSON.stringify(JSON.parse(value), null, 2), language: 'json' }
  } catch {
    return { text: value, language: 'text' }
  }
}

export function TraceSteps({ trace }: { trace: NodeTraceResult }) {
  const { locale } = useI18n()
  return <div className="space-y-0">
    {trace.steps.map((step, index) => {
      const data = step.data && typeof step.data === 'object' && !Array.isArray(step.data) ? step.data as Record<string, unknown> : { value: step.data }
      const failed = data.passed === false || data.status === 'error'
      const skipped = data.skipped === true
      return <div key={`${step.type}-${index}`} className="flex gap-3">
        <div className="flex flex-col items-center">
          <span className={`mt-1 ${failed ? 'text-red-600' : skipped ? 'text-[var(--muted)]' : 'text-green-600'}`}>
            {failed ? <CircleX size={20} /> : skipped ? <CircleMinus size={20} /> : <CheckCircle2 size={20} />}
          </span>
          {index < trace.steps.length - 1 ? <span className="my-1 w-px flex-1 bg-[var(--border)]" /> : null}
        </div>
        <section className="min-w-0 flex-1 pb-4 text-sm">
          <h4 className="font-semibold">{diagnosticText(locale, step.type)}</h4>
          <dl className="mt-2 grid grid-cols-1 gap-2 text-xs sm:grid-cols-2">
            {Object.entries(data).map(([key, value]) => {
              const code = typeof value === 'string' && (key === 'configFragment' || value.length > 120 || value.includes('\n')) ? traceText(value) : null
              return <div key={key} className={`min-w-0 ${code || hasStructuredCode(value) ? 'sm:col-span-2' : ''}`}>
              <dt className="text-[var(--muted)]">{diagnosticText(locale, key)}</dt>
              <dd className="min-w-0 break-all font-mono">{value && typeof value === 'object' ? <DiagnosticValue value={value} /> : code ? <ServerCodeBlock value={code.text} language={code.language.toUpperCase()} maxHeight={280} /> : String(value ?? '—')}</dd>
            </div>
            })}
          </dl>
        </section>
      </div>
    })}
  </div>
}

export function DiagnosticTrace({ traces, selectedId, open, onClose }: { traces: NodeTraceResult[]; selectedId?: string; open: boolean; onClose: () => void }) {
  const { locale } = useI18n()
  const [search, setSearch] = useState('')
  const [chosenId, setChosenId] = useState<string | null>(null)
  const selected = traces.find((trace, index) => (trace.traceId ?? String(index)) === (chosenId ?? selectedId))
  const filtered = traces.map((trace, index) => ({ trace, index })).filter(({ trace }) => trace.nodeName.toLowerCase().includes(search.trim().toLowerCase()))
  return <Modal open={open} title={diagnosticText(locale, 'traceTitle')} size="large" footer={null} onCancel={onClose} style={{ height: 'min(800px, calc(100dvh - 32px))' }} bodyStyle={{ display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
    <div className="grid min-h-0 flex-1 grid-cols-1 grid-rows-[minmax(5rem,30%)_minmax(0,1fr)] gap-3 md:grid-cols-[12rem_1fr] md:grid-rows-1">
      <aside className="flex min-h-0 flex-col gap-2">
        <Input value={search} onChange={(event) => setSearch(event.target.value)} placeholder={diagnosticText(locale, 'trace')} />
        <div className="min-h-0 flex-1 space-y-1 overflow-auto">
          {filtered.map(({ trace, index }) => <Button key={trace.traceId ?? index} size="small" variant={selected === trace ? 'primary' : 'text'} className="w-full justify-start truncate text-left" onClick={() => setChosenId(trace.traceId ?? String(index))}>{trace.nodeName}{trace.sourceIndex !== undefined ? ` · #${trace.sourceIndex + 1}` : ''}</Button>)}
        </div>
      </aside>
      <main className="min-h-0 min-w-0 overflow-auto">
        {selected ? <><h3 className="mb-3 break-all font-semibold">{selected.nodeName}</h3><TraceSteps trace={selected} /></> : <p className="text-sm text-[var(--muted)]">{diagnosticText(locale, traces.length ? 'trace' : 'traceEmpty')}</p>}
      </main>
    </div>
  </Modal>
}
