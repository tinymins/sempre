import { useState } from 'react'
import CodeMirror from '@uiw/react-codemirror'
import { json } from '@codemirror/lang-json'
import { Braces, Pencil, Plus, Trash2 } from 'lucide-react'
import { Modal, Select } from '@acme/components'
import { Badge, Button, Card, Field, Input } from '../../components/ui'
import { serverAPI, type ServerCustomNode, type ServerMember, type ServerSession } from './server-api'
import { useServerLocaleText } from './server-i18n'

const example = {
  name: 'edge', type: 'vless', server: 'edge.example.com', port: 443,
  uuid: '00000000-0000-0000-0000-000000000000', tls: true,
}

export function ServerCustomNodes({ session, nodes, members, onChange }: {
  session: ServerSession
  nodes: ServerCustomNode[]
  members: ServerMember[]
  onChange: (nodes: ServerCustomNode[]) => void
}) {
  const t = useServerLocaleText({
    title: '自定义节点库', detail: '这些节点可在订阅配置编辑器中复用。', add: '添加节点', shared: '共享', edit: '编辑', remove: '删除', empty: '还没有可复用节点。', addTitle: '添加自定义节点', editTitle: '编辑自定义节点', save: '保存', name: '名称', json: '节点 JSON', authorized: '授权的配置成员',
  }, {
    title: 'Custom node library', detail: 'These nodes can be reused from the subscription profile editor.', add: 'Add node', shared: 'Shared', edit: 'Edit', remove: 'Delete', empty: 'No reusable nodes yet.', addTitle: 'Add custom node', editTitle: 'Edit custom node', save: 'Save', name: 'Name', json: 'Node JSON', authorized: 'Authorized profile members',
  })
  const [editing, setEditing] = useState<ServerCustomNode | 'new' | null>(null)
  const [pending, setPending] = useState(false)
  const [name, setName] = useState('')
  const [content, setContent] = useState(`${JSON.stringify(example, null, 2)}\n`)
  const [authorized, setAuthorized] = useState<string[]>([])
  const [error, setError] = useState('')
  const open = (node: ServerCustomNode | 'new') => {
    setEditing(node)
    setName(node === 'new' ? '' : node.name)
    setContent(`${JSON.stringify(node === 'new' ? example : node.proxy, null, 2)}\n`)
    setAuthorized(node === 'new' ? [] : node.authorized_user_ids)
    setError('')
  }
  const save = async () => {
    if (!editing) return
    setPending(true)
    setError('')
    try {
      const proxy = JSON.parse(content) as Record<string, unknown>
      if (!proxy.name && name.trim()) proxy.name = name.trim()
      const result = await serverAPI<ServerCustomNode>(session, editing === 'new' ? '/custom-nodes' : `/custom-nodes/${editing.id}`, {
        method: editing === 'new' ? 'POST' : 'PUT',
        body: JSON.stringify({ name: name.trim() || String(proxy.name || ''), proxy, authorized_user_ids: authorized }),
      })
      onChange([result, ...nodes.filter((node) => node.id !== result.id)])
      setEditing(null)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setPending(false)
    }
  }
  const remove = async (node: ServerCustomNode) => {
    setError('')
    try {
      await serverAPI<void>(session, `/custom-nodes/${node.id}`, { method: 'DELETE' })
      onChange(nodes.filter((item) => item.id !== node.id))
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    }
  }
  return <>
    <Card className="space-y-4 p-5">
      <div className="flex items-center justify-between gap-3"><div><h2 className="font-semibold">{t.title}</h2><p className="text-sm text-[var(--muted)]">{t.detail}</p></div><Button onClick={() => open('new')}><Plus size={16} />{t.add}</Button></div>
      {error && !editing ? <p role="alert" className="text-sm text-red-600 dark:text-red-400">{error}</p> : null}
      <div className="grid gap-2 md:grid-cols-2">{nodes.map((node) => <div key={node.id} className="flex items-center justify-between gap-3 border-t border-[var(--border)] py-3"><div className="min-w-0"><div className="flex items-center gap-2"><span className="truncate font-medium">{node.name}</span><Badge>{String(node.proxy.type || '')}</Badge>{node.owner_id !== session.user.id ? <Badge tone="info">{t.shared}</Badge> : null}</div><p className="truncate font-mono text-xs text-[var(--muted)]">{String(node.proxy.server || '')}:{String(node.proxy.port || '')}</p></div>{node.owner_id === session.user.id ? <div className="flex gap-1"><Button size="icon" variant="ghost" aria-label={`${t.edit} ${node.name}`} onClick={() => open(node)}><Pencil size={15} /></Button><Button size="icon" variant="ghost" aria-label={`${t.remove} ${node.name}`} onClick={() => void remove(node)}><Trash2 size={15} /></Button></div> : null}</div>)}</div>
      {!nodes.length ? <p className="text-sm text-[var(--muted)]">{t.empty}</p> : null}
    </Card>
    <Modal open={Boolean(editing)} title={<span className="flex items-center gap-2"><Braces size={18} />{editing === 'new' ? t.addTitle : t.editTitle}</span>} width={900} okText={t.save} confirmLoading={pending} onOk={() => { void save(); return undefined }} onCancel={() => setEditing(null)} destroyOnClose>
      <div className="grid gap-4"><Field label={t.name}><Input value={name} onChange={(event) => setName(event.target.value)} /></Field><Field label={t.json}><div className="overflow-hidden rounded-md border border-[var(--border)]"><CodeMirror value={content} height="min(52vh, 520px)" extensions={[json()]} theme="dark" onChange={setContent} /></div></Field>{members.length ? <Field label={t.authorized}><Select mode="multiple" value={authorized} options={members.map((member) => ({ value: member.user_id, label: member.email }))} onChange={(value) => setAuthorized(value as string[])} /></Field> : null}{error ? <p role="alert" className="text-sm text-red-600 dark:text-red-400">{error}</p> : null}</div>
    </Modal>
  </>
}
