import { useState } from 'react'
import CodeMirror from '@uiw/react-codemirror'
import { json } from '@codemirror/lang-json'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Braces, Pencil, Plus, Trash2 } from 'lucide-react'
import { Modal } from '@acme/components'
import { api } from '../lib/api'
import { useIsMobile } from '../hooks'
import { useI18n } from '../lib/i18n'
import { parseJSONC } from '../lib/jsonc'
import { useSession } from '../lib/session'
import type { CustomNode } from '../lib/types'
import { Badge, Button, Card, EmptyState, Field, Input, PageTitle, Spinner } from '../components/ui'

const exampleNode = {
  name: 'edge',
  type: 'vless',
  server: 'edge.example.com',
  port: 443,
  uuid: '00000000-0000-0000-0000-000000000000',
  tls: true,
  servername: 'edge.example.com',
}

export function CustomNodes() {
  const { t } = useI18n()
  const { session } = useSession()
  const queryClient = useQueryClient()
  const [editorTarget, setEditorTarget] = useState<CustomNode | 'new' | null>(null)
  const [editorOpen, setEditorOpen] = useState(false)
  const [editorGeneration, setEditorGeneration] = useState(0)
  const [notice, setNotice] = useState('')
  const nodes = useQuery({ queryKey: ['custom-nodes'], queryFn: () => api<{ nodes: CustomNode[] }>(session!, '/custom-nodes') })
  const remove = useMutation({
    mutationFn: (id: string) => api(session!, `/custom-nodes/${id}`, { method: 'DELETE' }),
    onSuccess: () => { setNotice(t('operationDone')); queryClient.invalidateQueries({ queryKey: ['custom-nodes'] }) },
    onError: (error) => setNotice(error.message),
  })
  const openEditor = (target: CustomNode | 'new') => {
    setEditorTarget(target)
    setEditorGeneration((current) => current + 1)
    setEditorOpen(true)
  }
  const finishEditorClose = (open: boolean) => {
    if (!open) setEditorTarget(null)
  }
  return <div className="space-y-5">
    <PageTitle title={t('customNodes')}><Button variant="primary" onClick={() => openEditor('new')}><Plus size={16} />{t('addNode')}</Button></PageTitle>
    {notice ? <div className="border-l-2 border-emerald-500 bg-emerald-500/8 px-3 py-2 text-sm">{notice}</div> : null}
    <Card className="overflow-hidden">
      {nodes.isLoading ? <div className="grid min-h-48 place-items-center"><Spinner /></div> : nodes.data?.nodes.length ? <div className="overflow-x-auto"><table className="w-full min-w-[680px] text-left text-sm"><thead className="bg-[var(--surface-hover)] text-xs text-[var(--muted)]"><tr><th className="px-4 py-3 font-medium">{t('profileName')}</th><th className="px-4 py-3 font-medium">{t('type')}</th><th className="px-4 py-3 font-medium">{t('host')}</th><th className="px-4 py-3 font-medium">ID</th><th className="w-28" /></tr></thead><tbody>{nodes.data.nodes.map((node) => <tr key={node.id} className="border-t border-[var(--border)]"><td className="px-4 py-3 font-medium">{node.name}</td><td className="px-4 py-3"><Badge>{String(node.proxy.type || '')}</Badge></td><td className="px-4 py-3 font-mono text-xs">{String(node.proxy.server || '')}:{String(node.proxy.port || '')}</td><td className="px-4 py-3 font-mono text-xs text-[var(--muted)]">{node.id}</td><td className="px-4 py-2"><div className="flex justify-end gap-1"><Button size="icon" variant="ghost" title={t('editNode')} onClick={() => openEditor(node)}><Pencil size={15} /></Button><Button size="icon" variant="ghost" title={t('remove')} onClick={() => remove.mutate(node.id)}><Trash2 size={15} /></Button></div></td></tr>)}</tbody></table></div> : <EmptyState title={t('noData')} detail={t('noDataDetail')} action={<Button onClick={() => openEditor('new')}><Plus size={16} />{t('addNode')}</Button>} />}
    </Card>
    {editorTarget ? <NodeEditor key={editorGeneration} open={editorOpen} node={editorTarget === 'new' ? undefined : editorTarget} onClose={() => setEditorOpen(false)} afterOpenChange={finishEditorClose} onSaved={() => { setEditorOpen(false); setNotice(t('operationDone')); queryClient.invalidateQueries({ queryKey: ['custom-nodes'] }) }} /> : null}
  </div>
}

function NodeEditor({ open, node, onClose, onSaved, afterOpenChange }: { open: boolean; node?: CustomNode; onClose: () => void; onSaved: () => void; afterOpenChange: (open: boolean) => void }) {
  const { t } = useI18n()
  const { session } = useSession()
  const isMobile = useIsMobile()
  const [name, setName] = useState(node?.name || '')
  const [content, setContent] = useState(() => `${JSON.stringify(node?.proxy || exampleNode, null, 2)}\n`)
  const [error, setError] = useState('')
  const save = useMutation({
    mutationFn: () => {
      const proxy = parseJSONC<Record<string, unknown>>(content)
      if (!proxy.name && name) proxy.name = name
      const body = { name: name || String(proxy.name || ''), proxy }
      return api(session!, node ? `/custom-nodes/${node.id}` : '/custom-nodes', { method: node ? 'PUT' : 'POST', body: JSON.stringify(body) })
    },
    onSuccess: onSaved,
    onError: (cause) => setError(cause.message),
  })
  const handleSave = () => {
    setError('')
    try {
      parseJSONC(content)
      save.mutate()
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause))
    }
  }
  return (
    <Modal
      open={open}
      title={<span className="flex items-center gap-2"><Braces size={18} className="text-emerald-600" />{node ? t('editNode') : t('addNode')}</span>}
      size={isMobile ? 'full' : 'default'}
      width={isMobile ? undefined : 900}
      okText={t('save')}
      cancelText={t('cancel')}
      onOk={() => {
        handleSave()
        return undefined
      }}
      onCancel={onClose}
      afterOpenChange={afterOpenChange}
      confirmLoading={save.isPending}
      cancelButtonProps={{ disabled: save.isPending }}
      maskClosable={!save.isPending}
      keyboard={!save.isPending}
      closable={!save.isPending}
      destroyOnClose
      centered={!isMobile}
    >
      <div className="grid gap-4">
        <Field label={t('profileName')}><Input value={name} onChange={(event) => setName(event.target.value)} /></Field>
        <Field label={t('nodeJSON')}><div className="overflow-hidden rounded-md border border-[var(--border)]"><CodeMirror value={content} height="min(58vh, 560px)" extensions={[json()]} theme="dark" onChange={setContent} /></div></Field>
        {error ? <p className="text-sm text-red-600">{error}</p> : null}
      </div>
    </Modal>
  )
}
