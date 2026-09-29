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
    if (!tag || tags.includes(tag)) return
    edit(tags.length, tag)
    setNewTag('')
  }
  const finishEdit = () => {
    if (editingIndex === null) return
    const next = editingTag.trim()
    if (next && next !== tags[editingIndex] && !tags.some((tag, index) => index !== editingIndex && tag === next)) edit(editingIndex, next)
    setEditingIndex(null)
  }
  return <div className="space-y-3">
    {error ? <p role="alert" className="text-sm text-red-600">{t('filter.invalid')}</p> : null}
    <div className="flex flex-wrap gap-2">
      {tags.map((tag, index) => <span key={`${index}-${tag}`} className="inline-flex items-center gap-1 rounded border border-[var(--border)] bg-[var(--surface)] px-2 py-1 text-sm">
        {editingIndex === index ? <Input autoFocus value={editingTag} onChange={(event) => setEditingTag(event.target.value)} onBlur={finishEdit} onKeyDown={(event) => { if (event.key === 'Enter') finishEdit(); if (event.key === 'Escape') setEditingIndex(null) }} className="w-36" /> : readOnly ? tag : <Button size="small" variant="text" onClick={() => { setEditingIndex(index); setEditingTag(tag) }}>{tag}</Button>}
        {!readOnly ? <Button size="small" variant="text" icon={<X size={13} />} aria-label={t('filter.remove', { tag })} onClick={() => edit(index, undefined)} /> : null}
      </span>)}
      {tags.length === 0 && !error ? <span className="text-sm text-[var(--muted)]">{t('filter.empty')}</span> : null}
    </div>
    {!readOnly ? <div className="flex max-w-md gap-2"><Input value={newTag} disabled={Boolean(error)} placeholder={t('filter.add')} onChange={(event) => setNewTag(event.target.value)} onKeyDown={(event) => { if (event.key === 'Enter') { event.preventDefault(); add() } }} /><Button icon={<Plus size={14} />} disabled={Boolean(error) || !newTag.trim()} onClick={add}>{t('filter.add')}</Button></div> : null}
    {!readOnly && error ? <label className="block space-y-1 text-sm">{t('filter.advanced')}
      <CodeEditor height={180} value={value} onChange={(next) => onChange?.(next)} ariaLabel={t('filter.advanced')} />
    </label> : null}
  </div>
}
