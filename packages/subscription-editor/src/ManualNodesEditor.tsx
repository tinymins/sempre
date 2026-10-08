import { Button, Checkbox, CodeEditor, Empty, Form, Modal, Select, Tag } from '@acme/components'
import { DndContext, KeyboardSensor, PointerSensor, closestCenter, useSensor, useSensors } from '@dnd-kit/core'
import { SortableContext, arrayMove, sortableKeyboardCoordinates, useSortable, verticalListSortingStrategy } from '@dnd-kit/sortable'
import { CSS } from '@dnd-kit/utilities'
import { GripVertical } from 'lucide-react'
import { parse, type ParseError } from 'jsonc-parser'
import { useState } from 'react'
import { useEditorI18n as useI18n } from './i18n'
import type { ConfigDraft } from './model'
import { useEditorLayout } from './layout'

export interface EditorNode { id: string; name: string; label: string }

export function ManualNodesEditor({ readOnly, draft, nodes: assignedNodes, update }: { readOnly?: boolean; draft: Pick<ConfigDraft, 'servers' | 'selectedCustomNodeIds'>; nodes: EditorNode[]; update: (patch: Partial<ConfigDraft>) => void }) {
  const { t, number } = useI18n()
  const page = useEditorLayout() === 'page'
  const [manualOpen, setManualOpen] = useState(false)
  const [manualDraft, setManualDraft] = useState('')
  const [manualError, setManualError] = useState(false)
  const [order, setOrder] = useState(draft.selectedCustomNodeIds)
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 6 } }), useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }))
  const nodeIds = [...new Set([...order, ...draft.selectedCustomNodeIds, ...assignedNodes.map(node => node.id)])]
    .filter(id => assignedNodes.some(node => node.id === id) || draft.selectedCustomNodeIds.includes(id))
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
  const select = (id: string, checked: boolean) => {
    if (readOnly) return
    update({ selectedCustomNodeIds: nodeIds.filter(nodeId => nodeId === id ? checked : draft.selectedCustomNodeIds.includes(nodeId)) })
  }
  return (
    <div className="space-y-4">
      {page ? <Form.Item label={t('editor.selectedNodes')} tooltip={t('editor.nodesHelp')}><Select mode="multiple" value={draft.selectedCustomNodeIds} options={assignedNodes.map(node => ({ value: node.id, label: node.label, tagLabel: node.name }))} disabled={readOnly} onChange={next => { if (!readOnly) update({ selectedCustomNodeIds: next as string[] }) }} showSearch placeholder={t('editor.nodesPlaceholder')} /></Form.Item> : <><div className="flex items-center gap-2 text-sm"><span>{t('editor.selectedNodes')}</span><span className="text-[var(--muted)]">{number(draft.selectedCustomNodeIds.length)} / {number(nodeIds.length)}</span></div>
      <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={({ active, over }) => {
        if (readOnly || !over || active.id === over.id) return
        const next = arrayMove(nodeIds, nodeIds.indexOf(String(active.id)), nodeIds.indexOf(String(over.id)))
        setOrder(next)
        update({ selectedCustomNodeIds: next.filter(id => draft.selectedCustomNodeIds.includes(id)) })
      }}><SortableContext items={nodeIds} strategy={verticalListSortingStrategy}>
        <div className="space-y-2">{nodeIds.map((id, index) => <SortableNode key={id} id={id} index={index} node={assignedNodes.find(node => node.id === id)} checked={draft.selectedCustomNodeIds.includes(id)} disabled={readOnly} onChange={checked => select(id, checked)} />)}</div>
      </SortableContext></DndContext>
      {nodeIds.length === 0 ? <Empty description={t('common.noData')} /> : null}</>}
      <div className={page ? 'mt-6 flex items-center gap-2' : 'flex items-center gap-2 border-t border-[var(--border)] pt-4 text-sm'}>
        <span className={page ? 'text-sm text-[var(--text-secondary)]' : undefined}>{t('editor.manualServers')}</span>
        {page ? <Tag>{manualCount}</Tag> : <span className="rounded bg-[var(--surface)] px-2 py-0.5">{number(manualCount)}</span>}
        <Button size="small" variant="link" onClick={openManual}>{t('common.edit')}</Button>
      </div>
      <Modal open={manualOpen} title={t('editor.manualServers')} onCancel={() => setManualOpen(false)} onOk={saveManual} okText={t('common.save')} cancelText={t('common.cancel')} size="large" width={page ? 900 : undefined} style={page ? undefined : { height: 'min(760px, calc(100dvh - 32px))' }} bodyStyle={page ? undefined : { display: 'flex', flexDirection: 'column' }} destroyOnClose>
        <div className={page ? undefined : 'flex min-h-[20rem] flex-1 flex-col'}><CodeEditor readOnly={readOnly} value={manualDraft} onChange={(next) => { setManualDraft(next); setManualError(false) }} ariaLabel={t('editor.manualServers')} appearance={page ? 'plain' : 'panel'} height={page ? 'calc(100vh - 280px)' : '100%'} /></div>
        {manualError ? <p role="alert" className="mt-2 text-sm text-red-600">{t('editor.invalidJsonc', { field: t('editor.manualServers') })}</p> : null}
      </Modal>
    </div>
  )
}

function SortableNode({ id, index, node, checked, disabled, onChange }: { id: string; index: number; node?: EditorNode; checked: boolean; disabled?: boolean; onChange: (checked: boolean) => void }) {
  const { t, number } = useI18n()
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id, disabled })
  return <div ref={setNodeRef} style={{ transform: CSS.Transform.toString(transform), transition, opacity: isDragging ? 0.6 : 1 }} className="flex min-w-0 items-center gap-3 rounded-md border border-[var(--border)] bg-[var(--surface)] px-2 py-2 text-sm">
    <Button size="small" variant="text" disabled={disabled} icon={<GripVertical size={16} />} className="touch-none shrink-0" aria-label={t('editor.reorderNode', { index: number(index + 1) })} {...attributes} {...listeners} />
    <Checkbox checked={checked} disabled={disabled} onChange={event => onChange(event.target.checked)} className="min-w-0 flex-1"><span className="break-all">{node?.label ?? id}</span></Checkbox>
  </div>
}
