import { Collapse } from '@acme/components'

export function DiagnosticValue({ value, depth = 0 }: { value: unknown; depth?: number }) {
  if (value === null || typeof value !== 'object') return <span className="break-all font-mono text-xs">{String(value ?? 'null')}</span>
  if (depth > 8) return <span className="text-xs">…</span>
  const entries = Object.entries(value)
  if (!entries.length) return <span className="font-mono text-xs">{Array.isArray(value) ? '[]' : '{}'}</span>
  return <Collapse size="small" items={entries.map(([key, child]) => ({
    key,
    label: <span className="font-mono text-xs">{key}{child && typeof child === 'object' ? ` (${Object.keys(child).length})` : `: ${String(child ?? 'null')}`}</span>,
    children: <DiagnosticValue value={child} depth={depth + 1} />,
  }))} />
}
