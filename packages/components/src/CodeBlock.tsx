import { useId, type CSSProperties } from 'react'
import { Button } from './Button'
import { CodePanel } from './CodePanel'
import { useToast } from './Toast'

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
  return <CodePanel
    title={title}
    language={language}
    className={className}
    maxHeight={maxHeight}
    scrollLabel={title ?? language ?? 'Code'}
    actions={copyable ? <CopyAction value={value} copyLabel={copyLabel} copiedLabel={copiedLabel} copyErrorLabel={copyErrorLabel} /> : undefined}
  >
    <pre className={wrap ? 'acme-code-block acme-code-block--wrap' : 'acme-code-block'}><code>{value}</code></pre>
  </CodePanel>
}

function CopyAction({ value, copyLabel, copiedLabel, copyErrorLabel }: {
  value: string
  copyLabel: string
  copiedLabel: string
  copyErrorLabel: string
}) {
  const toast = useToast()
  const key = useId()

  async function copy() {
    try {
      await navigator.clipboard.writeText(value)
      toast.success({ content: copiedLabel, key })
    } catch {
      toast.error({ content: copyErrorLabel, key })
    }
  }

  return <Button size="small" variant="text" onClick={() => void copy()}>{copyLabel}</Button>
}
