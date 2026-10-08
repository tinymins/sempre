import { lazy, Suspense } from 'react'
import { CodePanel } from './CodePanel'

const CodeEditorRuntime = lazy(() => import('./CodeEditorRuntime'))

export interface CodeEditorProps {
  value: string
  onChange?: (value: string) => void
  readOnly?: boolean
  height?: string | number
  language?: string
  ariaLabel?: string
  appearance?: 'panel' | 'plain'
}

export function CodeEditor({ value, onChange, readOnly = false, height = 320, language = 'json', ariaLabel, appearance = 'panel' }: CodeEditorProps) {
  const fallback = appearance === 'plain' ? <div style={{ height }} /> : <CodePanel language={language} padded={false} className={height === '100%' ? 'acme-code-panel--fill' : undefined}><div className={height === '100%' ? 'absolute inset-0' : undefined} style={{ height }} /></CodePanel>
  return <Suspense fallback={fallback}>
    <CodeEditorRuntime value={value} onChange={onChange} readOnly={readOnly} height={height} language={language} ariaLabel={ariaLabel} appearance={appearance} />
  </Suspense>
}
