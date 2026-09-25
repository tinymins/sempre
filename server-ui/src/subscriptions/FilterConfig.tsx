import { Button, Input, TextArea } from '@acme/components'
import { applyEdits, modify, parse, type ParseError } from 'jsonc-parser'
import { Plus, X } from 'lucide-react'
import { useState } from 'react'
import { useI18n } from '../i18n/provider'

function readTags(value: string): { tags: string[]; error: boolean } {
  if (!value.trim()) return { tags: [], error: false }
  const errors: ParseError[] = []
  const parsed: unknown = parse(value, errors, { allowTrailingComma: true })
  if (errors.length || !Array.isArray(parsed) || parsed.some((tag) => typeof tag !== 'string')) {
    return { tags: [], error: true }
  }
  return { tags: parsed as string[], error: false }
}

export function FilterConfig({ value, readOnly = false, onChange }: { value: string; readOnly?: boolean; onChange?: (value: string) => void }) {
  const { t } = useI18n()
  const [newTag, setNewTag] = useState('')
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
  return <div className="space-y-3">
    {error ? <p role="alert" className="text-sm text-red-600">{t('filter.invalid')}</p> : null}
    <div className="flex flex-wrap gap-2">
      {tags.map((tag, index) => <span key={`${index}-${tag}`} className="inline-flex items-center gap-1 rounded border border-[var(--border)] bg-[var(--surface)] px-2 py-1 text-sm">
        {tag}{!readOnly ? <Button size="small" variant="text" icon={<X size={13} />} aria-label={t('filter.remove', { tag })} onClick={() => edit(index, undefined)} /> : null}
      </span>)}
      {tags.length === 0 && !error ? <span className="text-sm text-[var(--muted)]">{t('filter.empty')}</span> : null}
    </div>
    {!readOnly ? <div className="flex max-w-md gap-2"><Input value={newTag} disabled={Boolean(error)} placeholder={t('filter.add')} onChange={(event) => setNewTag(event.target.value)} onKeyDown={(event) => { if (event.key === 'Enter') { event.preventDefault(); add() } }} /><Button icon={<Plus size={14} />} disabled={Boolean(error) || !newTag.trim()} onClick={add}>{t('filter.add')}</Button></div> : null}
    {!readOnly ? <label className="block space-y-1 text-sm">{t('filter.advanced')}
      <TextArea rows={5} value={value} onChange={(event) => onChange?.(event.target.value)} className="font-mono text-xs" />
    </label> : null}
  </div>
}
