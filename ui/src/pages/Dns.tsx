import { useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Download, Plus, RefreshCw, Save, Trash2 } from 'lucide-react'
import { Alert, Button, Card, Empty, Input, InputNumber, Popover, Select, Switch, Table, Tabs, Tag, type TableColumn } from '@acme/components'
import { DnsUpstreamsInput } from '../features/dns/DnsUpstreamsInput'
import type { DnsFrontendStatus, DnsRewrite, DnsSettings, DnsSettingsResponse } from '../features/dns/types'
import { api } from '../lib/api'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import { compareNumber, compareText } from '../lib/sort'

interface DnsQueryEvent {
  time: number
  client: string
  name: string
  type: string
  decision: string
  answers: string[]
  upstream: string
  latency_ms: number
  detail: string
  error?: string
}

const emptyRewrite = (): DnsRewrite => ({ id: crypto.randomUUID(), enabled: true, domain: '', type: 'A', answer: '', ttl: 300, comment: '' })

export function Dns() {
  const { locale } = useI18n()
  const { session } = useSession()
  const queryClient = useQueryClient()
  const zh = locale === 'zh-CN'
  const [draft, setDraft] = useState<DnsSettings | null>(null)
  const [filter, setFilter] = useState('')
  const [rewrite, setRewrite] = useState<DnsRewrite>(emptyRewrite)
  const settings = useQuery({
    queryKey: ['dns', 'settings'],
    queryFn: () => api<DnsSettingsResponse>(session!, '/dns/settings'),
    enabled: Boolean(session),
    refetchInterval: 5000,
  })
  const queries = useQuery({
    queryKey: ['dns', 'queries'],
    queryFn: () => api<{ queries: DnsQueryEvent[] }>(session!, '/dns/queries'),
    enabled: Boolean(session),
    refetchInterval: 2000,
  })
  const save = useMutation({
    mutationFn: (candidate: DnsSettings) => api(session!, '/dns/settings', { method: 'PUT', body: JSON.stringify(candidate) }),
    onSuccess: () => {
      setDraft(null)
      queryClient.invalidateQueries({ queryKey: ['dns'] })
      queryClient.invalidateQueries({ queryKey: ['system'] })
    },
  })
  const clear = useMutation({
    mutationFn: () => api(session!, '/dns/queries', { method: 'DELETE' }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['dns', 'queries'] }),
  })
  const current = draft ?? settings.data?.settings
  const visibleQueries = useMemo(() => {
    const needle = filter.trim().toLowerCase()
    return (queries.data?.queries ?? []).filter((item) => !needle || `${item.name} ${item.client} ${item.answers.join(' ')} ${item.detail}`.toLowerCase().includes(needle))
  }, [filter, queries.data?.queries])
  const addRewrite = () => {
    if (!current || !rewrite.domain.trim() || !rewrite.answer.trim()) return
    const next = current.rewrites.filter((item) => item.id !== rewrite.id)
    setDraft({ ...current, rewrites: [...next, { ...rewrite, domain: rewrite.domain.trim(), answer: rewrite.answer.trim() }] })
    setRewrite(emptyRewrite())
  }
  const queryColumns = useMemo<Array<TableColumn<DnsQueryEvent>>>(() => [
    { title: zh ? '时间' : 'Time', dataIndex: 'time', width: 180, sorter: (left, right) => compareNumber(left.time, right.time), render: (value) => new Date(Number(value)).toLocaleString() },
    { title: zh ? '客户端' : 'Client', dataIndex: 'client', width: 130, sorter: (left, right) => compareText(left.client, right.client) },
    { title: zh ? '域名' : 'Name', dataIndex: 'name', minWidth: 220, sorter: (left, right) => compareText(left.name, right.name) },
    { title: zh ? '类型' : 'Type', dataIndex: 'type', width: 80, sorter: (left, right) => compareText(left.type, right.type) },
    { title: zh ? '决策' : 'Decision', dataIndex: 'decision', width: 100, sorter: (left, right) => compareText(left.decision, right.decision), render: (value) => <Tag color={value === 'local' ? 'green' : value === 'rewrite' ? 'blue' : value === 'reject' || value === 'error' ? 'red' : 'orange'}>{String(value)}</Tag> },
    { title: zh ? '应答' : 'Answers', dataIndex: 'answers', width: 340, ellipsis: true, sorter: (left, right) => compareText(left.answers.join(' '), right.answers.join(' ')), render: (value) => <DnsAnswerSummary answers={value as string[]} zh={zh} /> },
    { title: zh ? '上游' : 'Upstream', dataIndex: 'upstream', minWidth: 170, sorter: (left, right) => compareText(left.upstream, right.upstream) },
    { title: zh ? '耗时' : 'Latency', dataIndex: 'latency_ms', width: 90, sorter: (left, right) => left.latency_ms - right.latency_ms, render: (value) => `${value} ms` },
    { title: '', key: 'action', width: 60, render: (_value, item) => <Button size="small" variant="text" title={zh ? '添加重写' : 'Add rewrite'} onClick={() => setRewrite({ ...emptyRewrite(), domain: item.name.replace(/\.$/, ''), type: item.type === 'AAAA' ? 'AAAA' : 'A' })}><Plus size={14} /></Button> },
  ], [zh])
  const rewriteColumns = useMemo<Array<TableColumn<DnsRewrite>>>(() => [
    { title: zh ? '启用' : 'Enabled', dataIndex: 'enabled', width: 80, sorter: (left, right) => Number(left.enabled) - Number(right.enabled), render: (value, item) => <Switch size="small" checked={Boolean(value)} onChange={(enabled) => current && setDraft({ ...current, rewrites: current.rewrites.map((rule) => rule.id === item.id ? { ...rule, enabled } : rule) })} /> },
    { title: zh ? '域名' : 'Domain', dataIndex: 'domain', minWidth: 220, sorter: (left, right) => compareText(left.domain, right.domain) },
    { title: zh ? '类型' : 'Type', dataIndex: 'type', width: 90, sorter: (left, right) => compareText(left.type, right.type) },
    { title: zh ? '应答' : 'Answer', dataIndex: 'answer', minWidth: 220, sorter: (left, right) => compareText(left.answer, right.answer) },
    { title: 'TTL', dataIndex: 'ttl', width: 90, sorter: (left, right) => left.ttl - right.ttl },
    { title: zh ? '备注' : 'Comment', dataIndex: 'comment', minWidth: 160, sorter: (left, right) => compareText(left.comment, right.comment) },
    { title: '', key: 'action', width: 60, render: (_value, item) => <Button size="small" variant="text" title={zh ? '删除' : 'Delete'} onClick={() => current && setDraft({ ...current, rewrites: current.rewrites.filter((rule) => rule.id !== item.id) })}><Trash2 size={14} /></Button> },
  ], [current, zh])
  if (!current) return <div className="p-8 text-sm text-[var(--muted)]">{zh ? '正在加载 DNS 设置…' : 'Loading DNS settings…'}</div>
  const status = settings.data?.status
  const tabs = [
    { key: 'queries', label: zh ? '查询日志' : 'Query log', children: <QueryLog filter={filter} setFilter={setFilter} rows={visibleQueries} columns={queryColumns} clear={() => clear.mutate()} exporting={() => exportQueries(visibleQueries)} zh={zh} /> },
    { key: 'rewrites', label: zh ? 'DNS 重写' : 'DNS rewrites', children: <Rewrites current={current} rewrite={rewrite} setRewrite={setRewrite} add={addRewrite} columns={rewriteColumns} zh={zh} /> },
    { key: 'settings', label: zh ? '设置与状态' : 'Settings & status', children: <Settings current={current} setDraft={setDraft} status={status} zh={zh} /> },
  ]
  return <div className="space-y-5">
    <div className="flex min-h-10 items-start justify-between gap-4">
      <div><h1 className="text-xl font-semibold">DNS</h1><p className="mt-1 text-sm text-[var(--muted)]">{zh ? '设备级前置 DNS；核心 DNS 仍由当前订阅配置。' : 'Device-level DNS frontend. Core DNS remains owned by the active subscription.'}</p></div>
      <div className="flex gap-2"><Button icon={<RefreshCw size={16} />} onClick={() => { settings.refetch(); queries.refetch() }}>{zh ? '刷新' : 'Refresh'}</Button><Button variant="primary" icon={<Save size={16} />} loading={save.isPending} onClick={() => save.mutate(current)}>{zh ? '保存' : 'Save'}</Button></div>
    </div>
    {save.isError ? <Alert type="error" showIcon message={save.error instanceof Error ? save.error.message : String(save.error)} /> : null}
    {save.isSuccess ? <Alert type="success" showIcon message={zh ? '前置 DNS 设置已保存；接管开关变化时会暂存核心接线配置。' : 'Frontend DNS settings saved. Takeover changes stage the required core plumbing.'} /> : null}
    <Card className="!rounded-lg" bodyStyle={{ padding: '1rem' }}><Tabs items={tabs} defaultActiveKey="queries" destroyInactiveTabPane={false} /></Card>
  </div>
}

function QueryLog({ filter, setFilter, rows, columns, clear, exporting, zh }: { filter: string; setFilter: (value: string) => void; rows: DnsQueryEvent[]; columns: Array<TableColumn<DnsQueryEvent>>; clear: () => void; exporting: () => void; zh: boolean }) {
  return <div className="space-y-3 pt-4"><div className="flex flex-wrap gap-2"><Input className="min-w-64 flex-1" value={filter} placeholder={zh ? '筛选域名、客户端、应答或原因' : 'Filter name, client, answer, or reason'} onChange={(event) => setFilter(event.target.value)} /><Button icon={<Download size={15} />} onClick={exporting}>{zh ? '导出' : 'Export'}</Button><Button danger icon={<Trash2 size={15} />} onClick={clear}>{zh ? '清空' : 'Clear'}</Button></div><Table<DnsQueryEvent> rowKey={(item) => `${item.time}-${item.client}-${item.name}`} size="middle" pagination={{ pageSize: 50 }} columns={columns} dataSource={rows} scroll={{ x: 1300 }} locale={{ emptyText: <Empty description={zh ? '暂无 DNS 查询' : 'No DNS queries'} /> }} /></div>
}

function DnsAnswerSummary({ answers, zh }: { answers: string[]; zh: boolean }) {
  if (!answers.length) return '-'
  const targets = answerTargets(answers)
  const summary = `${targets.slice(0, 2).join(', ')}${answers.length > 2 ? ` · ${zh ? `共 ${answers.length} 条` : `${answers.length} records`}` : ''}`
  return <Popover trigger="click" placement="bottomLeft" fitViewport title={zh ? `完整应答（${answers.length} 条）` : `Full answers (${answers.length})`} popupClassName="z-[9999] max-w-3xl overflow-hidden rounded-lg border border-black/[0.06] bg-[var(--surface)] p-3 shadow-lg dark:border-white/[0.08]" content={<div className="max-h-80 space-y-1 overflow-auto font-mono text-xs leading-5">{answers.map((answer, index) => <div key={`${index}-${answer}`} className="break-all">{answer}</div>)}</div>}><Button size="small" variant="text" className="max-w-full !justify-start !px-1 font-mono font-normal" aria-label={`${zh ? '查看完整应答' : 'View full answers'}: ${summary}`}><span className="truncate">{summary}</span></Button></Popover>
}

function answerTargets(answers: string[]) {
  const records = answers.map((answer, index) => {
    const match = /\sIN\s([A-Z0-9]+)\s+(.+)$/i.exec(answer)
    const type = match?.[1]?.toUpperCase()
    return { index, priority: type === 'A' || type === 'AAAA' ? 0 : 1, value: (match?.[2] ?? answer).trim().replace(/\.$/, '') }
  }).sort((left, right) => left.priority - right.priority || left.index - right.index)
  return [...new Set(records.map((record) => record.value))]
}

function Rewrites({ current, rewrite, setRewrite, add, columns, zh }: { current: DnsSettings; rewrite: DnsRewrite; setRewrite: (value: DnsRewrite) => void; add: () => void; columns: Array<TableColumn<DnsRewrite>>; zh: boolean }) {
  return <div className="space-y-4 pt-4"><div className="grid gap-2 rounded-md border border-[var(--border)] p-3 md:grid-cols-6"><Input value={rewrite.domain} placeholder={zh ? '域名或 *.example.com' : 'Domain or *.example.com'} onChange={(event) => setRewrite({ ...rewrite, domain: event.target.value })} /><Select value={rewrite.type} options={['A', 'AAAA', 'CNAME'].map((value) => ({ value, label: value }))} onChange={(type) => setRewrite({ ...rewrite, type })} /><Input className="md:col-span-2" value={rewrite.answer} placeholder={zh ? 'IP 或目标域名' : 'IP or target name'} onChange={(event) => setRewrite({ ...rewrite, answer: event.target.value })} /><InputNumber className="w-full" min={0} value={rewrite.ttl} onChange={(ttl) => setRewrite({ ...rewrite, ttl: ttl ?? 300 })} /><Button variant="primary" icon={<Plus size={15} />} onClick={add}>{zh ? '添加' : 'Add'}</Button><Input className="md:col-span-6" value={rewrite.comment} placeholder={zh ? '备注（可选）' : 'Comment (optional)'} onChange={(event) => setRewrite({ ...rewrite, comment: event.target.value })} /></div><Table<DnsRewrite> rowKey="id" size="middle" pagination={false} columns={columns} dataSource={current.rewrites} scroll={{ x: 900 }} locale={{ emptyText: <Empty description={zh ? '暂无 DNS 重写' : 'No DNS rewrites'} /> }} /></div>
}

function Settings({ current, setDraft, status, zh }: { current: DnsSettings; setDraft: (value: DnsSettings) => void; status?: DnsFrontendStatus; zh: boolean }) {
  const captured = status?.original_upstreams ?? []
  return <div className="space-y-5 pt-4"><div className="grid gap-3 md:grid-cols-4"><Metric label={zh ? '前置 DNS' : 'DNS frontend'} value={status?.running ? (zh ? '运行中' : 'Running') : status?.enabled ? (zh ? '待启动' : 'Pending') : (zh ? '未启用' : 'Disabled')} /><Metric label={zh ? '核心 DNS 模式' : 'Core DNS mode'} value={status?.mode || '-'} /><Metric label={zh ? '核心 DNS' : 'Core DNS'} value={status?.core_dns_healthy ? (zh ? '正常' : 'Healthy') : '-'} /><Metric label={zh ? '国内域名规则' : 'Domestic domains'} value={String(status?.domestic_domain_count ?? 0)} /></div>{status?.last_error ? <Alert type="error" showIcon message={status.last_error} /> : null}<div className="grid gap-3 md:grid-cols-2"><SettingSwitch title={zh ? '启用前置 DNS' : 'Enable DNS frontend'} detail={zh ? '直连规则使用直连 DNS，其余查询进入当前核心。' : 'Resolve direct rules through direct DNS and send the rest to the active core.'} checked={current.enabled} onChange={(enabled) => setDraft({ ...current, enabled })} /><SettingSwitch title={zh ? '前置层拒绝 HTTPS 记录' : 'Reject HTTPS records at frontend'} detail={zh ? '仅影响前置层，不修改当前 Profile 的核心 DNS 设置。' : 'Affects only the frontend and does not modify the active profile DNS.'} checked={current.reject_https} onChange={(reject_https) => setDraft({ ...current, reject_https })} /><SettingSwitch title={zh ? '记录 DNS 查询' : 'Record DNS queries'} checked={current.query_log_enabled} onChange={(query_log_enabled) => setDraft({ ...current, query_log_enabled })} /><label className="flex items-center gap-3 rounded-md border border-[var(--border)] p-3 text-sm"><span className="flex-1">{zh ? '最多保留条数' : 'Maximum entries'}</span><InputNumber min={100} max={20000} value={current.query_log_max_entries} onChange={(query_log_max_entries) => setDraft({ ...current, query_log_max_entries: query_log_max_entries ?? 2000 })} /></label></div><DnsUpstreamsInput key={current.revision} upstreams={current.direct_upstreams} onChange={(direct_upstreams) => setDraft({ ...current, direct_upstreams })} zh={zh} /><div className="rounded-md border border-[var(--border)] p-3 text-xs leading-5 text-[var(--muted)]"><div>{zh ? '核心入口：' : 'Core upstream: '}{status?.core_upstream || '-'}</div><div>{zh ? '当前直连上游：' : 'Active direct upstreams: '}{status?.direct_upstreams?.join(', ') || '-'}</div><div>{zh ? '原始 DNS（只读）：' : 'Original DNS (read-only): '}{captured.join(', ') || '-'}</div><div>{zh ? '国内规则：内置 domains-min' : 'Domestic rules: built-in domains-min'}</div></div></div>
}

function SettingSwitch({ title, detail, checked, onChange }: { title: string; detail?: string; checked: boolean; onChange: (value: boolean) => void }) { return <div className="flex items-center justify-between gap-3 rounded-md border border-[var(--border)] p-3"><div><div className="text-sm font-medium">{title}</div>{detail ? <div className="mt-1 text-xs text-[var(--muted)]">{detail}</div> : null}</div><Switch checked={checked} onChange={onChange} /></div> }

function Metric({ label, value }: { label: string; value: string }) { return <div className="rounded-md border border-[var(--border)] p-3"><div className="text-xs text-[var(--muted)]">{label}</div><div className="mt-1 font-medium">{value}</div></div> }

function exportQueries(rows: DnsQueryEvent[]) {
  const blob = new Blob([JSON.stringify(rows, null, 2)], { type: 'application/json' })
  const url = URL.createObjectURL(blob)
  const anchor = document.createElement('a')
  anchor.href = url
  anchor.download = `sempre-dns-${new Date().toISOString().replace(/[:.]/g, '-')}.json`
  anchor.click()
  URL.revokeObjectURL(url)
}
