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

export default function CodeEditorRuntime({ value, onChange, readOnly = false, height = 320, language = 'json', ariaLabel }: CodeEditorProps) {
  const theme = useCodeEditorTheme()

  return <CodePanel language={language} padded={false} className={height === '100%' ? 'acme-code-panel--fill' : undefined}>
    <Editor
      height={height}
      language={language}
      value={value}
      theme={theme}
      beforeMount={(instance: Monaco) => {
        instance.languages.json.jsonDefaults.setDiagnosticsOptions({ validate: true, allowComments: true, trailingCommas: 'ignore' })
        defineCodeEditorThemes(instance)
      }}
      onChange={(next) => { if (!readOnly) onChange?.(next ?? '') }}
      options={{ automaticLayout: true, ariaLabel, fontSize: 13, fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace', lineHeight: 20, padding: { top: 12, bottom: 12 }, minimap: { enabled: false }, readOnly, scrollBeyondLastLine: false, tabSize: 2, wordWrap: 'on' }}
    />
  </CodePanel>
}
