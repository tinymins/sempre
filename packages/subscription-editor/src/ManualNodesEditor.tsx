import { Button, CodeEditor, Form, Modal, Table, Tag } from '@acme/components'
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
  const nodeIds = [...new Set([...order, ...draft.selectedCustomNodeIds, ...assignedNodes.map(node => node.id)])]
    .filter(id => assignedNodes.some(node => node.id === id) || draft.selectedCustomNodeIds.includes(id))
  const nodes = nodeIds.map(id => assignedNodes.find(node => node.id === id) ?? { id, name: id, label: id })
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
  const nodeTable = <Table<EditorNode> rowKey="id" dataSource={nodes} pagination={false}
    columns={[{ title: t('common.name'), dataIndex: 'label', render: label => <span className="break-all">{label}</span> }]}
    rowSelection={{ selectedRowKeys: draft.selectedCustomNodeIds, getCheckboxProps: () => ({ disabled: readOnly }), onChange: keys => {
      if (!readOnly) update({ selectedCustomNodeIds: nodeIds.filter(id => keys.includes(id)) })
    } }}
    onReorder={next => {
      if (readOnly) return
      const ids = next.map(node => node.id)
      setOrder(ids)
      update({ selectedCustomNodeIds: ids.filter(id => draft.selectedCustomNodeIds.includes(id)) })
    }} sortDisabled={readOnly} locale={{ emptyText: t('common.noData') }} />
  return (
    <div className="space-y-4">
      {page ? <Form.Item label={t('editor.selectedNodes')} tooltip={t('editor.nodesHelp')}>{nodeTable}</Form.Item> : <><div className="flex items-center gap-2 text-sm"><span>{t('editor.selectedNodes')}</span><span className="text-[var(--muted)]">{number(draft.selectedCustomNodeIds.length)} / {number(nodeIds.length)}</span></div>{nodeTable}</>}
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
