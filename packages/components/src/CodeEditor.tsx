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
}

export function CodeEditor({ value, onChange, readOnly = false, height = 320, language = 'json', ariaLabel }: CodeEditorProps) {
  return <Suspense fallback={<CodePanel language={language} padded={false} className={height === '100%' ? 'acme-code-panel--fill' : undefined}><div style={{ height }} /></CodePanel>}>
    <CodeEditorRuntime value={value} onChange={onChange} readOnly={readOnly} height={height} language={language} ariaLabel={ariaLabel} />
  </Suspense>
}
