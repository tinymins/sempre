import { ServerCodeBlock } from '../ServerCodeBlock'

export function hasStructuredCode(value: unknown): boolean {
  return value !== null && typeof value === 'object' && Object.keys(value).length > 0
}

export function DiagnosticValue({ value }: { value: unknown }) {
  const content = JSON.stringify(value, null, 2) ?? String(value)
  if (!hasStructuredCode(value)) return <code className="rounded border border-[var(--code-border)] bg-[var(--code-surface)] px-1.5 py-0.5 font-mono text-xs text-[var(--code-text)]">{content}</code>
  return <ServerCodeBlock value={content} language="JSON" maxHeight={320} />
}
