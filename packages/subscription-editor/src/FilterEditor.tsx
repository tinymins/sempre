import { CodeEditor, TagListEditor } from '@acme/components'
import { applyEdits, modify, parse, type ParseError } from 'jsonc-parser'
import { useEditorI18n as useI18n } from './i18n'
import { useEditorLayout } from './layout'

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
  const page = useEditorLayout() === 'page'
  const { tags, error } = readTags(value)
  const edit = (index: number, next: string | undefined) => {
    if (readOnly || error) return
    const text = value.trim() ? value : '[]'
    onChange?.(applyEdits(text, modify(text, [index], next, { formattingOptions: { insertSpaces: true, tabSize: 2, eol: '\n' } })))
  }
  return <div className="space-y-3">
    {error ? <p role="alert" className="text-sm text-red-600">{t('filter.invalid')}</p> : null}
    <TagListEditor value={tags} readOnly={readOnly} disabled={error}
      addLabel={t('filter.add')} removeLabel={(tag) => t('filter.remove', { tag })} emptyLabel={t('filter.empty')}
      onChange={(_tags, change) => edit(change.index, change.value)} />
    {!readOnly && error ? <label className="block space-y-1 text-sm">{t('filter.advanced')}
      <CodeEditor appearance={page ? 'plain' : 'panel'} height={180} value={value} onChange={(next) => onChange?.(next)} ariaLabel={t('filter.advanced')} />
    </label> : null}
  </div>
}
