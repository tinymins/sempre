import { Button, CodeEditor, Modal, Select } from '@acme/components'
import { ArrowDown, ArrowUp } from 'lucide-react'
import { parse, type ParseError } from 'jsonc-parser'
import { useState } from 'react'
import { useEditorI18n as useI18n } from './i18n'
import type { ConfigDraft } from './model'

export interface EditorNode { id: string; name: string; label: string }

export function ManualNodesEditor({ readOnly, draft, nodes: assignedNodes, update }: { readOnly?: boolean; draft: Pick<ConfigDraft, 'servers' | 'selectedCustomNodeIds'>; nodes: EditorNode[]; update: (patch: Partial<ConfigDraft>) => void }) {
  const { t, number } = useI18n()
  const [manualOpen, setManualOpen] = useState(false)
  const [manualDraft, setManualDraft] = useState('')
  const [manualError, setManualError] = useState(false)
  const parsed = parse(draft.servers || '[]') as unknown
  const manualCount = Array.isArray(parsed) ? parsed.length : 0
  const openManual = () => { setManualDraft(draft.servers || '[]'); setManualError(false); setManualOpen(true) }
  const saveManual = (): undefined => {
    if (readOnly) return undefined
    const errors: ParseError[] = []
    const next: unknown = parse(manualDraft, errors, { allowTrailingComma: true })
    if (errors.length || !Array.isArray(next)) { setManualError(true); return undefined }
    update({ servers: manualDraft || null })
    setManualOpen(false)
    return undefined
  }
  const move = (index: number, direction: -1 | 1) => {
    const target = index + direction
    if (target < 0 || target >= draft.selectedCustomNodeIds.length) return
    const ids = [...draft.selectedCustomNodeIds]
    ;[ids[index], ids[target]] = [ids[target], ids[index]]
    update({ selectedCustomNodeIds: ids })
  }
  return (
    <div className="space-y-4">
      <label className="block space-y-1 text-sm">{t('editor.selectedNodes')}
        <Select mode="multiple" value={draft.selectedCustomNodeIds} options={assignedNodes.map((node) => ({ value: node.id, label: node.label, tagLabel: node.name }))} onChange={(next) => update({ selectedCustomNodeIds: next as string[] })} showSearch className="w-full" />
      </label>
      {draft.selectedCustomNodeIds.map((id, index) => {
        const node = assignedNodes.find((item) => item.id === id)
        return <div key={id} className="flex items-center justify-between gap-2 rounded border border-[var(--border)] px-2 py-1 text-sm">
          <span>{node?.name ?? id}</span><span className="flex gap-1"><Button size="small" icon={<ArrowUp size={14} />} aria-label={t('editor.moveNodeUp', { index: number(index + 1) })} disabled={index === 0} onClick={() => move(index, -1)} /><Button size="small" icon={<ArrowDown size={14} />} aria-label={t('editor.moveNodeDown', { index: number(index + 1) })} disabled={index === draft.selectedCustomNodeIds.length - 1} onClick={() => move(index, 1)} /></span>
        </div>
      })}
      <div className="flex items-center gap-2 border-t border-[var(--border)] pt-4 text-sm">
        <span>{t('editor.manualServers')}</span>
        <span className="rounded bg-[var(--surface)] px-2 py-0.5">{number(manualCount)}</span>
        <Button size="small" variant="link" onClick={openManual}>{t('common.edit')}</Button>
      </div>
      <Modal open={manualOpen} title={t('editor.manualServers')} onCancel={() => setManualOpen(false)} onOk={saveManual} okText={t('common.save')} cancelText={t('common.cancel')} size="large" style={{ height: 'min(760px, calc(100dvh - 32px))' }} bodyStyle={{ display: 'flex', flexDirection: 'column' }} destroyOnClose>
        <div className="min-h-[20rem] flex-1"><CodeEditor readOnly={readOnly} value={manualDraft} onChange={(next) => { setManualDraft(next); setManualError(false) }} ariaLabel={t('editor.manualServers')} height="100%" /></div>
        {manualError ? <p role="alert" className="mt-2 text-sm text-red-600">{t('editor.invalidJsonc', { field: t('editor.manualServers') })}</p> : null}
      </Modal>
    </div>
  )
}
