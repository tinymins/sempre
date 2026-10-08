import { Checkbox, CodeEditor } from '@acme/components'
import type { ReactNode } from 'react'
import { FilterEditor } from './FilterEditor'
import { useEditorI18n } from './i18n'
import { inheritedFields, type InheritedField } from './model'
import { useEditorLayout } from './layout'

interface Props {
  readOnly?: boolean
  field: InheritedField
  value: string | null | undefined
  inherited: boolean
  defaultValue: string | null | undefined
  onChange: (value: string) => void
  onInheritanceChange: (inherited: boolean) => void
  children?: (value: string, readOnly: boolean) => ReactNode
}

export function InheritedSection({ readOnly, field, value, inherited, defaultValue, onChange, onInheritanceChange, children }: Props) {
  const { t } = useEditorI18n()
  const page = useEditorLayout() === 'page'
  const label = t(page ? `editor.field.${field}` : inheritedFields[field].label)
  const displayed = (inherited ? defaultValue : value) ?? ''
  return <section className={page ? field === 'filter' ? 'mt-4 border-t border-gray-200 pt-4 dark:border-zinc-700' : '' : 'flex min-h-0 flex-1 flex-col gap-3'}>
    <div className={page ? 'mb-2 flex items-center justify-between' : 'flex items-center justify-between gap-3'}>
      <span className={page ? field === 'filter' ? 'text-sm font-medium' : undefined : 'text-sm font-medium'}>{label}</span>
      <Checkbox disabled={readOnly || (inherited && defaultValue == null)} checked={inherited} onChange={event => onInheritanceChange(event.target.checked)}>{t('editor.inherit')}</Checkbox>
    </div>
    {inherited && !page ? <p className="text-xs text-[var(--muted)]">{t('editor.inheritHint')}</p> : null}
    {inherited && defaultValue == null ? <p className="text-sm text-[var(--muted)]">{t('common.unavailable')}</p> : children ? children(displayed, inherited) : field === 'filter' ?
      <FilterEditor value={displayed} readOnly={readOnly || inherited} onChange={onChange} /> :
      page ? <CodeEditor ariaLabel={label} value={displayed} readOnly={readOnly || inherited} onChange={onChange} appearance="plain" height="calc(100vh - 280px)" /> : <div className="flex min-h-[20rem] flex-1 flex-col"><CodeEditor ariaLabel={label} value={displayed} readOnly={readOnly || inherited} onChange={onChange} height="100%" /></div>}
  </section>
}
