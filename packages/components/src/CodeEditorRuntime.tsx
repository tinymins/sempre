import Editor, { loader, type Monaco } from '@monaco-editor/react'
import * as monaco from 'monaco-editor'
import EditorWorker from 'monaco-editor/esm/vs/editor/editor.worker.js?worker'
import JsonWorker from 'monaco-editor/esm/vs/language/json/json.worker.js?worker'
import type { CodeEditorProps } from './CodeEditor'

Object.assign(self, { MonacoEnvironment: {
  getWorker(_workerId: string, label: string) {
    return label === 'json' ? new JsonWorker() : new EditorWorker()
  },
} })
loader.config({ monaco })

export default function CodeEditorRuntime({ value, onChange, readOnly = false, height = 320, language = 'json', ariaLabel }: CodeEditorProps) {
  return <div className="overflow-hidden rounded border border-[var(--border)]">
    <Editor
      height={height}
      language={language}
      value={value}
      theme="vs-dark"
      beforeMount={(instance: Monaco) => instance.languages.json.jsonDefaults.setDiagnosticsOptions({ validate: true, allowComments: true, trailingCommas: 'ignore' })}
      onChange={(next) => { if (!readOnly) onChange?.(next ?? '') }}
      options={{ automaticLayout: true, ariaLabel, fontSize: 13, minimap: { enabled: false }, readOnly, scrollBeyondLastLine: false, tabSize: 2, wordWrap: 'on' }}
    />
  </div>
}
