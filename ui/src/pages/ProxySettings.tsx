import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Pencil, Plus, Trash2, Zap } from 'lucide-react'
import { Checkbox, Modal, Popconfirm, Select, Table, Tag, TextArea, useToast, type TableColumn } from '@acme/components'
import { defaultManualProxyContent } from '@acme/subscription-editor'
import { Button, Card, Field, Input, PageTitle } from '../components/ui'
import { ManualProxyFields } from '../features/proxy-settings/ManualProxyFields'
import { fetchProxiesKey, useFetchProxies, type FetchProxyInput, type FetchProxySummary } from '../features/proxy-settings/useFetchProxies'
import { api } from '../lib/api'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import type { CustomNode } from '../lib/types'

type Mode = 'custom-node' | 'import' | 'manual'
const EMPTY: FetchProxyInput = { name: '', content: '', custom_node_id: null, enabled: true }

export function ProxySettings() {
  const { locale } = useI18n()
  const { session } = useSession()
  const toast = useToast()
  const queryClient = useQueryClient()
  const label = (zh: string, en: string) => locale === 'zh-CN' ? zh : en
  const proxies = useFetchProxies()
  const [editing, setEditing] = useState<{ id: string | null; input: FetchProxyInput } | null>(null)
  const refresh = () => queryClient.invalidateQueries({ queryKey: fetchProxiesKey })
  const failed = (error: Error) => toast.error(error.message)
  const test = useMutation({
    mutationFn: (id: string) => api<{ elapsed_ms: number }>(session!, `/fetch-proxies/${id}/test`, { method: 'POST' }),
    onSuccess: ({ elapsed_ms }) => toast.success(label(`连接成功，耗时 ${elapsed_ms} ms`, `Connected in ${elapsed_ms} ms`)),
    onError: failed,
  })
  const remove = useMutation({
    mutationFn: (id: string) => api(session!, `/fetch-proxies/${id}`, { method: 'DELETE' }),
    onSuccess: refresh,
    onError: failed,
  })
  const open = useMutation({
    mutationFn: (id: string) => api<FetchProxyInput & { id: string }>(session!, `/fetch-proxies/${id}`),
    onSuccess: ({ id, name, content, custom_node_id, enabled }) => setEditing({ id, input: { name, content, custom_node_id, enabled } }),
    onError: failed,
  })
  const columns: Array<TableColumn<FetchProxySummary>> = [
    { title: label('名称', 'Name'), dataIndex: 'name', key: 'name', render: (_value, item) => <div className="flex items-center gap-2"><span>{item.name}</span>{item.enabled ? null : <Tag>{label('已停用', 'Disabled')}</Tag>}</div> },
    { title: label('协议', 'Protocol'), key: 'protocol', width: 140, render: (_value, item) => item.protocol ?? <span title={item.error ?? undefined} className="text-red-600">{label('配置无效', 'Invalid')}</span> },
    { title: label('来源', 'Source'), key: 'source', width: 130, render: (_value, item) => item.custom_node_id ? label('自定义节点', 'Custom node') : label('独立配置', 'Standalone') },
    { title: label('引用', 'Used by'), dataIndex: 'reference_count', key: 'reference_count', width: 80 },
    {
      title: label('操作', 'Actions'), key: 'actions', width: 170, render: (_value, item) => <div className="flex items-center gap-1">
        <Button variant="ghost" title={label('测试', 'Test')} aria-label={label('测试', 'Test')} disabled={!item.enabled || test.isPending} onClick={() => test.mutate(item.id)}><Zap size={15} /></Button>
        <Button variant="ghost" title={label('编辑', 'Edit')} aria-label={label('编辑', 'Edit')} onClick={() => open.mutate(item.id)}><Pencil size={15} /></Button>
        <Popconfirm title={label('删除这个代理？', 'Delete this proxy?')} onConfirm={() => remove.mutate(item.id)}><Button variant="ghost" title={label('删除', 'Delete')} aria-label={label('删除', 'Delete')}><Trash2 size={15} /></Button></Popconfirm>
      </div>,
    },
  ]
  return <div className="space-y-5">
    <PageTitle title={label('代理设置', 'Proxy Settings')} detail={label('拉取订阅时使用的代理。只在下载时临时启动，在订阅地址里为每个来源选择。', 'Proxies for downloading subscriptions. They start only for a download; choose one per source under the subscription address.')}>
      <Button onClick={() => setEditing({ id: null, input: EMPTY })}><Plus size={16} />{label('新增代理', 'Add proxy')}</Button>
    </PageTitle>
    <Card className="!p-0"><Table<FetchProxySummary> size="small" rowKey="id" loading={proxies.isLoading} dataSource={proxies.data?.items ?? []} columns={columns} pagination={false} scroll={{ x: 680 }} /></Card>
    {editing ? <ProxyEditor key={editing.id ?? 'new'} id={editing.id} initial={editing.input} label={label} onClose={() => setEditing(null)} onSaved={refresh} /> : null}
  </div>
}

function ProxyEditor({ id, initial, label, onClose, onSaved }: { id: string | null; initial: FetchProxyInput; label: (zh: string, en: string) => string; onClose: () => void; onSaved: () => Promise<void> }) {
  const { session } = useSession()
  const [input, setInput] = useState(initial)
  const [mode, setMode] = useState<Mode>(initial.custom_node_id ? 'custom-node' : 'import')
  const nodes = useQuery({ queryKey: ['custom-nodes'], queryFn: () => api<{ nodes: CustomNode[] }>(session!, '/custom-nodes') })
  const patch = (change: Partial<FetchProxyInput>) => setInput((current) => ({ ...current, ...change }))
  const save = useMutation({
    mutationFn: () => api(session!, id ? `/fetch-proxies/${id}` : '/fetch-proxies', {
      method: id ? 'PUT' : 'POST',
      body: JSON.stringify(mode === 'custom-node' ? { ...input, content: '' } : { ...input, custom_node_id: null }),
    }),
    onSuccess: async () => { await onSaved(); onClose() },
  })
  const incomplete = !input.name.trim() || (mode === 'custom-node' ? !input.custom_node_id : !input.content.trim())
  return <Modal open title={id ? label('编辑代理', 'Edit proxy') : label('新增代理', 'Add proxy')} okText={label('保存', 'Save')} cancelText={label('取消', 'Cancel')} confirmLoading={save.isPending} okButtonProps={{ disabled: incomplete }} onOk={() => { save.mutate(); return undefined }} onCancel={onClose}>
    <div className="space-y-4">
      {save.error ? <p role="alert" className="text-sm text-red-600">{save.error.message}</p> : null}
      <Field label={label('名称', 'Name')}><Input autoFocus value={input.name} onChange={(event) => patch({ name: event.target.value })} /></Field>
      <Field label={label('配置方式', 'Configuration')}><Select className="w-full" value={mode} options={[
        { value: 'custom-node', label: label('使用自定义节点', 'Use a custom node') },
        { value: 'import', label: label('粘贴节点配置', 'Paste a node') },
        { value: 'manual', label: label('手动填写', 'Fill in manually') },
      ]} onChange={(value) => {
        if (value === 'manual') patch({ content: defaultManualProxyContent() })
        setMode(value as Mode)
      }} /></Field>
      {mode === 'custom-node' ? <Field label={label('自定义节点', 'Custom node')} hint={label('节点内容变化后，代理自动使用新内容。', 'The proxy follows later changes to the node.')}><Select className="w-full" value={input.custom_node_id ?? undefined} placeholder={label('请选择', 'Select')} options={(nodes.data?.nodes ?? []).map((node) => ({ value: node.id, label: node.name }))} onChange={(value) => patch({ custom_node_id: String(value) })} /></Field> : null}
      {mode === 'import' ? <Field label={label('节点配置', 'Node')} hint={label('一个节点链接、Clash 节点或 sing-box outbound。', 'One node link, Clash node, or sing-box outbound.')}><TextArea rows={7} spellCheck={false} value={input.content} onChange={(event) => patch({ content: event.target.value })} /></Field> : null}
      {mode === 'manual' ? <ManualProxyFields disabled={save.isPending} label={label} onChange={(content) => patch({ content })} /> : null}
      <Checkbox checked={input.enabled} onChange={(event) => patch({ enabled: event.target.checked })}>{label('启用', 'Enabled')}</Checkbox>
    </div>
  </Modal>
}
