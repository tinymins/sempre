import { Button, Collapse, Tag } from '@acme/components'
import { CheckCircle2, CircleMinus, CircleX, LoaderCircle } from 'lucide-react'
import type { DebugStage, NodeTraceResult } from './diagnostic-types'
import { diagnosticText } from './diagnostic-locale'
import { useI18n } from '../i18n/provider'
import { DiagnosticValue } from './DiagnosticValue'

function stageKey(stage: DebugStage): string {
  return [stage.type, stage.sourceIndex ?? stage.sourceId ?? '', stage.attempt ?? ''].join(':')
}

export function convergeStages(stages: DebugStage[]): DebugStage[] {
  const positions = new Map<string, number>()
  const result: DebugStage[] = []
  for (const stage of stages) {
    if (stage.type === 'source-filter') {
      const fetchIndex = result.findIndex((item) => item.type === 'fetch' && item.sourceId === stage.sourceId && item.sourceIndex === stage.sourceIndex)
      if (fetchIndex >= 0) {
        result[fetchIndex] = {
          ...result[fetchIndex],
          nodesBeforeFilter: stage.nodesBeforeFilter,
          nodesAfterFilter: stage.nodesAfterFilter,
          filteredCount: stage.filteredCount,
        }
        continue
      }
    }
    const key = stageKey(stage)
    const index = positions.get(key)
    if (index === undefined) {
      positions.set(key, result.length)
      result.push(stage)
    } else {
      result[index] = { ...result[index], ...stage }
    }
  }
  return result
}

function detailValue(value: unknown): string {
  if (value === null) return '—'
  if (typeof value === 'boolean') return value ? '✓' : '—'
  if (typeof value === 'string' || typeof value === 'number') return String(value)
  return ''
}

const preferred = ['fetchMode', 'cacheTtlMinutes', 'cacheState', 'cached', 'httpStatus', 'format', 'parsedNodeCount', 'nodesBeforeFilter', 'nodesAfterFilter', 'filteredCount', 'nodeCount', 'fetchDurationMs', 'durationMs', 'attempt', 'maxAttempts']
const excluded = new Set(['type', 'status', 'sourceId', 'sourceIndex', 'sourceRemark', 'sourceLabel', 'message', 'rawText', 'decodedText', 'contentHash', 'nodeNames'])

export function DiagnosticTimeline({ stages, search = '', traces = [], onTrace }: { stages: DebugStage[]; search?: string; traces?: NodeTraceResult[]; onTrace?: (id: string) => void }) {
  const { locale, number } = useI18n()
  const query = search.trim().toLowerCase()
  const visible = convergeStages(stages).filter((stage) => !query || JSON.stringify(stage).toLowerCase().includes(query))
  return <div className="space-y-0">
    {visible.map((stage, index) => {
      const title = `${diagnosticText(locale, stage.type)}${stage.sourceIndex !== undefined ? ` #${number(stage.sourceIndex + 1)}` : ''}${stage.sourceLabel || stage.sourceRemark ? ` · ${stage.sourceLabel || stage.sourceRemark}` : ''}`
      const isSkipped = stage.status === 'skipped' || (stage.type === 'network' && stage.dnsTcpProbed === false)
      const color = stage.status === 'error' ? 'text-red-600' : stage.status === 'running' ? 'text-blue-600' : isSkipped ? 'text-[var(--muted)]' : 'text-green-600'
      const icon = stage.status === 'error' ? <CircleX size={20} /> : stage.status === 'running' ? <LoaderCircle size={20} className="animate-spin" /> : isSkipped ? <CircleMinus size={20} /> : <CheckCircle2 size={20} />
      const details = [...preferred, ...Object.keys(stage).filter((key) => !preferred.includes(key))]
        .filter((key, position, all) => all.indexOf(key) === position && !excluded.has(key) && stage[key] !== undefined)
      const stageTraces = stage.type === 'fetch' && stage.sourceId
        ? traces.map((trace, traceIndex) => ({ trace, traceIndex })).filter(({ trace }) => trace.sourceId === stage.sourceId && (stage.sourceIndex === undefined || trace.sourceIndex === stage.sourceIndex))
        : []
      const metrics = stage.type === 'fetch' || stage.type === 'source-filter'
        ? [
          ['httpStatus', stage.httpStatus === null ? null : stage.httpStatus],
          ['fetchDurationMs', stage.fetchDurationMs], ['format', stage.format],
          ['parsedNodeCount', stage.parsedNodeCount], ['nodesBeforeFilter', stage.nodesBeforeFilter],
          ['nodesAfterFilter', stage.nodesAfterFilter], ['filteredCount', stage.filteredCount ?? stage.filteredNodes],
        ] as const
          : stage.type === 'source' || stage.type === 'attempt' || stage.type === 'cache' || stage.type === 'fallback'
          ? [['httpStatus', stage.httpStatus], ['cacheState', stage.cacheState], ['durationMs', stage.durationMs]] as const
          : [['nodeCount', stage.nodeCount], ['cacheState', stage.cacheState]] as const
      const summaryFields = new Set<string>(metrics.map(([key]) => key))
      const remaining = details.filter((key) => !summaryFields.has(key))
      return <div key={stageKey(stage)} className="flex gap-3">
        <div className="flex flex-col items-center">
          <span className={`mt-1 ${color}`} aria-label={diagnosticText(locale, stage.status)}>{icon}</span>
          {index < visible.length - 1 ? <span className="my-1 w-px flex-1 bg-[var(--border)]" /> : null}
        </div>
        <div className="min-w-0 flex-1 pb-4">
          <div className="flex flex-wrap items-center gap-2 text-sm font-semibold">
            <span>{title}</span><span className={`text-xs font-normal ${color}`}>{diagnosticText(locale, stage.status)}</span>
            {stage.type === 'attempt' && stage.attempt !== undefined ? <span className="text-xs">#{number(stage.attempt)}</span> : null}
          </div>
          {stage.message ? <p className="mt-1 break-words text-xs text-[var(--muted)]">{stage.message}</p> : null}
          <div className="mt-1 flex flex-wrap gap-1.5">
            {metrics.filter(([, value]) => value !== undefined && value !== null).map(([key, value]) => <Tag key={key} color={key === 'filteredCount' ? 'orange' : key === 'cacheState' ? 'blue' : 'default'}>{diagnosticText(locale, key)}: {typeof value === 'number' ? number(value) : String(value)}</Tag>)}
          </div>
          {remaining.length || stageTraces.length || stage.rawText || stage.decodedText ? <Collapse size="small" items={[{ key: 'detail', label: diagnosticText(locale, 'details'), children: <div className="space-y-2"><dl className="grid grid-cols-1 gap-x-4 gap-y-2 text-xs sm:grid-cols-2">
            {remaining.map((key) => <div key={key} className="min-w-0"><dt className="text-[var(--muted)]">{diagnosticText(locale, key)}</dt><dd className="max-h-40 overflow-auto whitespace-pre-wrap break-all font-mono">{stage[key] && typeof stage[key] === 'object' ? <DiagnosticValue value={stage[key]} /> : detailValue(stage[key])}</dd></div>)}
            {stage.type === 'network' ? <p className="text-[var(--muted)] sm:col-span-2">{diagnosticText(locale, 'networkUnknown')}</p> : null}
          </dl>{stage.rawText ? <Collapse size="small" items={[{ key: 'raw', label: diagnosticText(locale, 'rawText'), children: <pre className="max-h-48 overflow-auto whitespace-pre-wrap break-all text-xs">{String(stage.rawText)}</pre> }]} /> : null}
          {stage.decodedText ? <Collapse size="small" items={[{ key: 'decoded', label: diagnosticText(locale, 'decodedText'), children: <pre className="max-h-48 overflow-auto whitespace-pre-wrap break-all text-xs">{String(stage.decodedText)}</pre> }]} /> : null}
          {stageTraces.length && onTrace ? <div className="flex max-h-32 flex-wrap gap-1 overflow-auto">{stageTraces.map(({ trace, traceIndex }) => <Button key={trace.traceId ?? traceIndex} size="small" variant="text" onClick={() => onTrace(trace.traceId ?? String(traceIndex))}>{trace.nodeName}</Button>)}</div> : null}</div> }]}/> : null}
        </div>
      </div>
    })}
    {!visible.length && stages.length ? <p className="text-xs text-[var(--muted)]">{diagnosticText(locale, 'searchEmpty')}</p> : null}
  </div>
}
