import { AutoComplete, Button, CodeEditor, Input } from '@acme/components'
import { Plus, Trash2 } from 'lucide-react'
import { useState } from 'react'
import { useEditorI18n } from './i18n'
import { editJsonc } from './jsonc'
import { useEditorLayout } from './layout'
import { readRuleList, renameRuleGroup, type RuleSource } from './rule-list'

interface Props {
  value: string
  readOnly?: boolean
  onChange: (value: string) => void
}

export function RuleListEditor({ value, readOnly, onChange }: Props) {
  const { t } = useEditorI18n()
  const page = useEditorLayout() === 'page'
  const groups = readRuleList(value)
  const edit = (path: (string | number)[], next: unknown) => {
    if (!readOnly && groups) onChange(editJsonc(value, path, next))
  }
  if (!groups) return <div className="space-y-3">
    <p role="alert" className="text-sm text-red-600">{t('rules.invalid')}</p>
    <CodeEditor ariaLabel={t('editor.tabRules')} value={value} readOnly={readOnly} onChange={onChange} height={320} appearance={page ? 'plain' : 'panel'} />
  </div>

  const addGroup = () => {
    let index = 1
    while (Object.hasOwn(groups, t('rules.newGroup', { index }))) index++
    edit([t('rules.newGroup', { index })], [{ name: '', url: '' }])
  }
  return <div className="space-y-4">
    <p className="text-xs text-[var(--muted)]">{t('rules.intro')}</p>
    {Object.entries(groups).map(([group, sources], index) => <RuleGroup key={index} group={group} sources={sources} readOnly={readOnly}
      groupIndex={index + 1} names={Object.keys(groups)} edit={edit}
      rename={next => { if (!readOnly) onChange(renameRuleGroup(value, group, next)) }} />)}
    {!Object.keys(groups).length ? <p className="text-sm text-[var(--muted)]">{t('rules.empty')}</p> : null}
    <Button disabled={readOnly} variant="dashed" block icon={<Plus size={15} />} onClick={addGroup}>{t('rules.addGroup')}</Button>
  </div>
}

interface GroupProps {
  group: string
  groupIndex: number
  sources: RuleSource[]
  names: string[]
  readOnly?: boolean
  edit: (path: (string | number)[], next: unknown) => void
  rename: (next: string) => void
}

function RuleGroup({ group, groupIndex, sources, names, readOnly, edit, rename }: GroupProps) {
  const { t } = useEditorI18n()
  const [draftName, setDraftName] = useState<{ group: string; value: string } | null>(null)
  const name = draftName?.group === group ? draftName.value : group
  const invalid = !name.trim() || (name.trim() !== group && names.includes(name.trim()))
  const commitName = () => {
    if (!readOnly && !invalid && name.trim() !== group) rename(name.trim())
  }
  return <section className="min-w-0 space-y-3 rounded-lg border border-[var(--border)] p-3 sm:p-4">
    <div className="flex items-start gap-2">
      <label className="min-w-0 flex-1 space-y-1 text-sm font-medium">{t('rules.outbound')}
        <Input aria-label={`${t('rules.outbound')} · ${groupIndex}`} disabled={readOnly} value={name} placeholder="PROXY / DIRECT"
          status={invalid ? 'error' : undefined} onChange={event => setDraftName({ group, value: event.target.value })} onBlur={commitName} onPressEnter={event => { event.preventDefault(); commitName() }} />
      </label>
      <Button disabled={readOnly} danger size="small" className="mt-6 shrink-0" icon={<Trash2 size={15} />} aria-label={t('rules.removeGroup', { group })} onClick={() => edit([group], undefined)} />
    </div>
    {invalid ? <p role="alert" className="text-sm text-red-600">{t('rules.groupError')}</p> : null}
    {sources.map((source, index) => <div key={index} className="space-y-3 rounded-md bg-black/[0.03] p-3 dark:bg-white/[0.03]">
      <div className="flex items-center justify-between gap-2">
        <span className="text-sm font-medium">{t('rules.source', { index: index + 1 })}</span>
        <Button disabled={readOnly} danger size="small" icon={<Trash2 size={14} />} aria-label={t('rules.removeSource', { group, index: index + 1 })} onClick={() => edit([group, index], undefined)} />
      </div>
      <div className="grid min-w-0 gap-3 sm:grid-cols-2">
        <label className="min-w-0 space-y-1 text-sm">{t('common.name')}
          <Input disabled={readOnly} value={source.name} onChange={event => edit([group, index, 'name'], event.target.value)} />
        </label>
        <label className="min-w-0 space-y-1 text-sm">{t('rules.url')}
          <Input disabled={readOnly} type="url" value={source.url} placeholder="https://example.com/rules" onChange={event => edit([group, index, 'url'], event.target.value)} />
        </label>
        <label className="min-w-0 space-y-1 text-sm">{t('rules.type')}
          <AutoComplete disabled={readOnly} value={source.type ?? ''} options={['domain', 'ipcidr', 'classical']} placeholder={t('rules.auto')} className="w-full" onChange={next => edit([group, index, 'type'], next || undefined)} />
        </label>
        <label className="min-w-0 space-y-1 text-sm">{t('rules.format')}
          <AutoComplete disabled={readOnly} value={source.format ?? ''} options={['yaml', 'text', 'binary', 'source', 'mrs']} placeholder={t('rules.auto')} className="w-full" onChange={next => edit([group, index, 'format'], next || undefined)} />
        </label>
      </div>
    </div>)}
    <Button disabled={readOnly} variant="dashed" block icon={<Plus size={15} />} onClick={() => edit([group, sources.length], { name: '', url: '' })}>{t('rules.addSource')}</Button>
  </section>
}
