import { lazy, Suspense } from 'react'

const CodeEditorRuntime = lazy(() => import('./CodeEditorRuntime'))

export interface CodeEditorProps {
  value: string
  onChange?: (value: string) => void
  readOnly?: boolean
  height?: string | number
  language?: string
  ariaLabel?: string
}

export function CodeEditor({ value, onChange, readOnly = false, height = 320, language = 'json', ariaLabel }: CodeEditorProps) {
  return <Suspense fallback={<div className="rounded border border-[var(--border)]" style={{ height }} />}>
    <CodeEditorRuntime value={value} onChange={onChange} readOnly={readOnly} height={height} language={language} ariaLabel={ariaLabel} />
  </Suspense>
}
