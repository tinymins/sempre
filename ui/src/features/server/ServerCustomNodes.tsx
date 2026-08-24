import { useState } from 'react'
import CodeMirror from '@uiw/react-codemirror'
import { json } from '@codemirror/lang-json'
import { Braces, Pencil, Plus, Trash2 } from 'lucide-react'
import { Modal, Select } from '@acme/components'
import { Badge, Button, Card, Field, Input } from '../../components/ui'
import { serverAPI, type ServerCustomNode, type ServerMember, type ServerSession } from './server-api'

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
      <div className="flex items-center justify-between gap-3"><div><h2 className="font-semibold">Custom node library</h2><p className="text-sm text-[var(--muted)]">Reusable nodes can be selected from the profile editor above.</p></div><Button onClick={() => open('new')}><Plus size={16} />Add node</Button></div>
      {error && !editing ? <p role="alert" className="text-sm text-red-600 dark:text-red-400">{error}</p> : null}
      <div className="grid gap-2 md:grid-cols-2">{nodes.map((node) => <div key={node.id} className="flex items-center justify-between gap-3 border-t border-[var(--border)] py-3"><div className="min-w-0"><div className="flex items-center gap-2"><span className="truncate font-medium">{node.name}</span><Badge>{String(node.proxy.type || '')}</Badge></div><p className="truncate font-mono text-xs text-[var(--muted)]">{String(node.proxy.server || '')}:{String(node.proxy.port || '')}</p></div><div className="flex gap-1"><Button size="icon" variant="ghost" aria-label={`Edit ${node.name}`} onClick={() => open(node)}><Pencil size={15} /></Button>{node.owner_id === session.user.id ? <Button size="icon" variant="ghost" aria-label={`Delete ${node.name}`} onClick={() => void remove(node)}><Trash2 size={15} /></Button> : null}</div></div>)}</div>
      {!nodes.length ? <p className="text-sm text-[var(--muted)]">No reusable nodes yet.</p> : null}
    </Card>
    <Modal open={Boolean(editing)} title={<span className="flex items-center gap-2"><Braces size={18} />{editing === 'new' ? 'Add custom node' : 'Edit custom node'}</span>} width={900} okText="Save" confirmLoading={pending} onOk={() => { void save(); return undefined }} onCancel={() => setEditing(null)} destroyOnClose>
      <div className="grid gap-4"><Field label="Name"><Input value={name} onChange={(event) => setName(event.target.value)} /></Field><Field label="Node JSON"><div className="overflow-hidden rounded-md border border-[var(--border)]"><CodeMirror value={content} height="min(52vh, 520px)" extensions={[json()]} theme="dark" onChange={setContent} /></div></Field>{members.length ? <Field label="Authorized profile members"><Select mode="multiple" value={authorized} options={members.map((member) => ({ value: member.user_id, label: member.email }))} onChange={(value) => setAuthorized(value as string[])} /></Field> : null}{error ? <p role="alert" className="text-sm text-red-600 dark:text-red-400">{error}</p> : null}</div>
    </Modal>
  </>
}
