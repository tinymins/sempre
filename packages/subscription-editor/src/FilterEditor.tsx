import { Button, CodeEditor, Input } from '@acme/components'
import { applyEdits, modify, parse, type ParseError } from 'jsonc-parser'
import { Plus, X } from 'lucide-react'
import { useState } from 'react'
import { useEditorI18n as useI18n } from './i18n'

function readTags(value: string): { tags: string[]; error: boolean } {
  if (!value.trim()) return { tags: [], error: false }
  const errors: ParseError[] = []
  const parsed: unknown = parse(value, errors, { allowTrailingComma: true })
  if (errors.length || !Array.isArray(parsed) || parsed.some((tag) => typeof tag !== 'string')) {
    return { tags: [], error: true }
  }
  return { tags: parsed as string[], error: false }
}

export function FilterEditor({ value, readOnly = false, onChange }: { value: string; readOnly?: boolean; onChange?: (value: string) => void }) {
  const { t } = useI18n()
  const [newTag, setNewTag] = useState('')
  const [isAdding, setIsAdding] = useState(false)
  const [editingIndex, setEditingIndex] = useState<number | null>(null)
  const [editingTag, setEditingTag] = useState('')
  const { tags, error } = readTags(value)
  const edit = (index: number, next: string | undefined) => {
    if (readOnly || error) return
    const text = value.trim() ? value : '[]'
    onChange?.(applyEdits(text, modify(text, [index], next, { formattingOptions: { insertSpaces: true, tabSize: 2, eol: '\n' } })))
  }
  const add = () => {
    const tag = newTag.trim()
    if (tag && !tags.includes(tag)) edit(tags.length, tag)
    setNewTag('')
    setIsAdding(false)
  }
  const finishEdit = () => {
    if (editingIndex === null) return
    const next = editingTag.trim()
    if (next && next !== tags[editingIndex] && !tags.some((tag, index) => index !== editingIndex && tag === next)) edit(editingIndex, next)
    setEditingIndex(null)
  }
  return <div className="space-y-3">
    {error ? <p role="alert" className="text-sm text-red-600">{t('filter.invalid')}</p> : null}
    <div className="flex flex-wrap items-center gap-2">
      {tags.map((tag, index) => <span key={`${index}-${tag}`} className={`inline-flex min-h-7 max-w-full items-center gap-1 rounded-md border border-[var(--border)] bg-[var(--input-bg)] px-2.5 py-1 text-sm leading-5 ${readOnly ? 'text-[var(--text-muted)]' : 'text-[var(--text-primary)]'}`}>
        {editingIndex === index ? <Input autoFocus value={editingTag} onChange={(event) => setEditingTag(event.target.value)} onBlur={finishEdit} onKeyDown={(event) => {
          if (event.key === 'Enter') { event.preventDefault(); finishEdit() }
          if (event.key === 'Escape') { event.stopPropagation(); setEditingIndex(null) }
        }} className="h-5 w-36 min-w-0 max-w-full rounded-none border-0 bg-transparent p-0" /> : readOnly ? <span className="min-w-0 [overflow-wrap:anywhere]">{tag}</span> : <Button variant="unstyled" className="min-w-0 text-left [overflow-wrap:anywhere]" onClick={() => { setEditingIndex(index); setEditingTag(tag) }}>{tag}</Button>}
        {!readOnly ? <Button variant="unstyled" className="size-4 shrink-0 rounded-sm text-[var(--text-muted)] transition-colors hover:text-red-500" icon={<X size={13} />} aria-label={t('filter.remove', { tag })} onClick={() => edit(index, undefined)} /> : null}
      </span>)}
      {!readOnly ? isAdding ? <Input autoFocus value={newTag} disabled={error} aria-label={t('filter.add')} placeholder={t('filter.add')} onChange={(event) => setNewTag(event.target.value)} onBlur={add} onKeyDown={(event) => {
        if (event.key === 'Enter') { event.preventDefault(); add() }
        if (event.key === 'Escape') { event.stopPropagation(); setNewTag(''); setIsAdding(false) }
      }} className="h-8 w-44 min-w-0 max-w-full border-[var(--accent)]" /> : <Button variant="dashed" className="h-8 shrink-0 px-2.5 text-[var(--text-muted)]" icon={<Plus size={14} />} aria-label={t('filter.add')} title={t('filter.add')} disabled={error} onClick={() => setIsAdding(true)} /> : null}
      {tags.length === 0 && !error && readOnly ? <span className="text-sm text-[var(--text-muted)]">{t('filter.empty')}</span> : null}
    </div>
    {!readOnly && error ? <label className="block space-y-1 text-sm">{t('filter.advanced')}
      <CodeEditor height={180} value={value} onChange={(next) => onChange?.(next)} ariaLabel={t('filter.advanced')} />
    </label> : null}
  </div>
}
