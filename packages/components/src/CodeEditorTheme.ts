import { useEffect, useState } from 'react'
import type { Monaco } from '@monaco-editor/react'

export function useCodeEditorTheme(): 'sempre-code-light' | 'sempre-code-dark' {
  const [dark, setDark] = useState(() => typeof document !== 'undefined' && document.documentElement.classList.contains('dark'))

  useEffect(() => {
    const root = document.documentElement
    const observer = new MutationObserver(() => setDark(root.classList.contains('dark')))
    observer.observe(root, { attributes: true, attributeFilter: ['class'] })
    return () => observer.disconnect()
  }, [])

  return dark ? 'sempre-code-dark' : 'sempre-code-light'
}

export function defineCodeEditorThemes(instance: Monaco): void {
  instance.editor.defineTheme('sempre-code-light', {
    base: 'vs', inherit: true, rules: [],
    colors: {
      'editor.background': '#f5f7fa',
      'editor.foreground': '#253247',
      'editorLineNumber.foreground': '#8a96a8',
    },
  })
  instance.editor.defineTheme('sempre-code-dark', {
    base: 'vs-dark', inherit: true, rules: [],
    colors: {
      'editor.background': '#111827',
      'editor.foreground': '#e5edf8',
      'editorLineNumber.foreground': '#77849a',
    },
  })
}
