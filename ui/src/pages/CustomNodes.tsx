import { useState } from 'react'
import CodeMirror from '@uiw/react-codemirror'
import { json } from '@codemirror/lang-json'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Braces, Pencil, Plus, Trash2, X } from 'lucide-react'
import { api } from '../lib/api'
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
  const [editing, setEditing] = useState<CustomNode | 'new' | null>(null)
  const [notice, setNotice] = useState('')
  const nodes = useQuery({ queryKey: ['custom-nodes'], queryFn: () => api<{ nodes: CustomNode[] }>(session!, '/custom-nodes') })
  const remove = useMutation({
    mutationFn: (id: string) => api(session!, `/custom-nodes/${id}`, { method: 'DELETE' }),
    onSuccess: () => { setNotice(t('operationDone')); queryClient.invalidateQueries({ queryKey: ['custom-nodes'] }) },
    onError: (error) => setNotice(error.message),
  })
  return <div className="space-y-5">
    <PageTitle title={t('customNodes')}><Button variant="primary" onClick={() => setEditing('new')}><Plus size={16} />{t('addNode')}</Button></PageTitle>
    {notice ? <div className="border-l-2 border-emerald-500 bg-emerald-500/8 px-3 py-2 text-sm">{notice}</div> : null}
    <Card className="overflow-hidden">
      {nodes.isLoading ? <div className="grid min-h-48 place-items-center"><Spinner /></div> : nodes.data?.nodes.length ? <div className="overflow-x-auto"><table className="w-full min-w-[680px] text-left text-sm"><thead className="bg-[var(--surface-hover)] text-xs text-[var(--muted)]"><tr><th className="px-4 py-3 font-medium">{t('profileName')}</th><th className="px-4 py-3 font-medium">{t('type')}</th><th className="px-4 py-3 font-medium">{t('host')}</th><th className="px-4 py-3 font-medium">ID</th><th className="w-28" /></tr></thead><tbody>{nodes.data.nodes.map((node) => <tr key={node.id} className="border-t border-[var(--border)]"><td className="px-4 py-3 font-medium">{node.name}</td><td className="px-4 py-3"><Badge>{String(node.proxy.type || '')}</Badge></td><td className="px-4 py-3 font-mono text-xs">{String(node.proxy.server || '')}:{String(node.proxy.port || '')}</td><td className="px-4 py-3 font-mono text-xs text-[var(--muted)]">{node.id}</td><td className="px-4 py-2"><div className="flex justify-end gap-1"><Button size="icon" variant="ghost" title={t('editNode')} onClick={() => setEditing(node)}><Pencil size={15} /></Button><Button size="icon" variant="ghost" title={t('remove')} onClick={() => remove.mutate(node.id)}><Trash2 size={15} /></Button></div></td></tr>)}</tbody></table></div> : <EmptyState title={t('noData')} detail={t('noDataDetail')} action={<Button onClick={() => setEditing('new')}><Plus size={16} />{t('addNode')}</Button>} />}
    </Card>
    {editing ? <NodeEditor node={editing === 'new' ? undefined : editing} onClose={() => setEditing(null)} onSaved={() => { setEditing(null); setNotice(t('operationDone')); queryClient.invalidateQueries({ queryKey: ['custom-nodes'] }) }} /> : null}
  </div>
}

function NodeEditor({ node, onClose, onSaved }: { node?: CustomNode; onClose: () => void; onSaved: () => void }) {
  const { t } = useI18n()
  const { session } = useSession()
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
  return <div className="fixed inset-0 z-50 grid place-items-center bg-black/45 p-4" onMouseDown={(event) => { if (event.target === event.currentTarget && !save.isPending) onClose() }}>
    <div role="dialog" aria-modal="true" className="flex max-h-[90vh] w-full max-w-3xl flex-col overflow-hidden rounded-lg border border-[var(--border)] bg-[var(--surface)] shadow-2xl">
      <div className="flex h-14 shrink-0 items-center border-b border-[var(--border)] px-4"><Braces size={18} className="mr-2 text-emerald-600" /><h2 className="text-sm font-semibold">{node ? t('editNode') : t('addNode')}</h2><Button className="ml-auto" size="icon" variant="ghost" title={t('close')} onClick={onClose}><X size={17} /></Button></div>
      <div className="grid min-h-0 flex-1 gap-4 overflow-y-auto p-4"><Field label={t('profileName')}><Input value={name} onChange={(event) => setName(event.target.value)} /></Field><Field label={t('nodeJSON')}><div className="overflow-hidden rounded-md border border-[var(--border)]"><CodeMirror value={content} height="min(58vh, 560px)" extensions={[json()]} theme="dark" onChange={setContent} /></div></Field>{error ? <p className="text-sm text-red-600">{error}</p> : null}</div>
      <div className="flex shrink-0 justify-end gap-2 border-t border-[var(--border)] p-4"><Button onClick={onClose}>{t('cancel')}</Button><Button variant="primary" disabled={save.isPending} onClick={() => { setError(''); try { parseJSONC(content); save.mutate() } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)) } }}>{save.isPending ? <Spinner /> : null}{t('save')}</Button></div>
    </div>
  </div>
}
