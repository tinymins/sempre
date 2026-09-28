import { useEffect, useState, type CSSProperties } from 'react'
import { Button } from './Button'
import { CodePanel } from './CodePanel'

export interface CodeBlockProps {
  value: string
  language?: string
  title?: string
  maxHeight?: CSSProperties['maxHeight']
  wrap?: boolean
  copyLabel?: string
  copiedLabel?: string
  copyErrorLabel?: string
  className?: string
  copyable?: boolean
}

export function CodeBlock({
  value,
  language,
  title,
  maxHeight = 420,
  wrap = false,
  copyLabel = 'Copy',
  copiedLabel = 'Copied',
  copyErrorLabel = 'Copy failed',
  className,
  copyable = true,
}: CodeBlockProps) {
  const [copyState, setCopyState] = useState<'idle' | 'copied' | 'error'>('idle')

  useEffect(() => {
    if (copyState === 'idle') return
    const timer = window.setTimeout(() => setCopyState('idle'), 2400)
    return () => window.clearTimeout(timer)
  }, [copyState])

  async function copy() {
    try {
      await navigator.clipboard.writeText(value)
      setCopyState('copied')
    } catch {
      setCopyState('error')
    }
  }

  const copyText = copyState === 'copied' ? copiedLabel : copyState === 'error' ? copyErrorLabel : copyLabel

  return <CodePanel
    title={title}
    language={language}
    className={className}
    maxHeight={maxHeight}
    scrollLabel={title ?? language ?? 'Code'}
    actions={copyable ? <Button size="small" variant="text" onClick={() => void copy()} aria-live="polite">{copyText}</Button> : undefined}
  >
    <pre className={wrap ? 'acme-code-block acme-code-block--wrap' : 'acme-code-block'}><code>{value}</code></pre>
  </CodePanel>
}
