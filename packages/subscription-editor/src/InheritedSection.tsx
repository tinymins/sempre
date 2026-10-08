import { Checkbox, CodeEditor } from '@acme/components'
import type { ReactNode } from 'react'
import { FilterEditor } from './FilterEditor'
import { useEditorI18n } from './i18n'
import { inheritedFields, type InheritedField } from './model'

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
  const label = t(inheritedFields[field].label)
  const displayed = (inherited ? defaultValue : value) ?? ''
  return <section className="flex min-h-0 flex-1 flex-col gap-3">
    <div className="flex items-center justify-between gap-3">
      <span className="text-sm font-medium">{label}</span>
      <Checkbox disabled={readOnly || (inherited && defaultValue == null)} checked={inherited} onChange={event => onInheritanceChange(event.target.checked)}>{t('editor.inherit')}</Checkbox>
    </div>
    {inherited ? <p className="text-xs text-[var(--muted)]">{t('editor.inheritHint')}</p> : null}
    {inherited && defaultValue == null ? <p className="text-sm text-[var(--muted)]">{t('common.unavailable')}</p> : children ? children(displayed, inherited) : field === 'filter' ?
      <FilterEditor value={displayed} readOnly={readOnly || inherited} onChange={onChange} /> :
      <div className="flex min-h-[20rem] flex-1 flex-col"><CodeEditor ariaLabel={label} value={displayed} readOnly={readOnly || inherited} onChange={onChange} height="100%" /></div>}
  </section>
}
