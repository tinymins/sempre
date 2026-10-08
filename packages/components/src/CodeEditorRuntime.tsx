import Editor, { loader, type Monaco } from '@monaco-editor/react'
import * as monaco from 'monaco-editor'
import EditorWorker from 'monaco-editor/esm/vs/editor/editor.worker.js?worker'
import JsonWorker from 'monaco-editor/esm/vs/language/json/json.worker.js?worker'
import type { CodeEditorProps } from './CodeEditor'
import { defineCodeEditorThemes, useCodeEditorTheme } from './CodeEditorTheme'
import { CodePanel } from './CodePanel'

Object.assign(self, { MonacoEnvironment: {
  getWorker(_workerId: string, label: string) {
    return label === 'json' ? new JsonWorker() : new EditorWorker()
  },
} })
loader.config({ monaco })

export default function CodeEditorRuntime({ value, onChange, readOnly = false, height = 320, language = 'json', ariaLabel, appearance = 'panel' }: CodeEditorProps) {
  const theme = useCodeEditorTheme()
  const plain = appearance === 'plain'
  const editor = <Editor
        height={height}
        language={language}
        value={value}
        theme={plain ? 'sempre-subscription' : theme}
        beforeMount={(instance: Monaco) => {
          instance.languages.json.jsonDefaults.setDiagnosticsOptions({ validate: true, allowComments: true, trailingCommas: 'ignore' })
          defineCodeEditorThemes(instance)
          instance.editor.defineTheme('sempre-subscription', { base: 'vs-dark', inherit: true, rules: [], colors: { 'editor.background': '#141414' } })
        }}
        onChange={(next) => { if (!readOnly) onChange?.(next ?? '') }}
        options={{ automaticLayout: true, ariaLabel, fontSize: plain ? 14 : 13, fontFamily: plain ? "Menlo, Monaco, 'Courier New', monospace" : 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace', lineHeight: plain ? 0 : 20, padding: { top: plain ? 0 : 12, bottom: plain ? 0 : 12 }, ...(plain ? { selectOnLineNumbers: true, renderControlCharacters: true, renderWhitespace: 'all' as const } : {}), minimap: { enabled: false }, readOnly, scrollBeyondLastLine: false, tabSize: 2, wordWrap: 'on' }}
      />
  if (plain) return <div className={`overflow-hidden rounded border ${readOnly ? 'border-gray-500 opacity-60 dark:border-gray-500' : 'border-gray-300 dark:border-gray-600'}`}>{editor}</div>
  return <CodePanel language={language} padded={false} className={height === '100%' ? 'acme-code-panel--fill' : undefined}>
    <div className={height === '100%' ? 'absolute inset-0' : undefined}>{editor}
    </div>
  </CodePanel>
}
