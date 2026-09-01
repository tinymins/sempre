import { useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Download, Plus, RefreshCw, Save, Trash2 } from 'lucide-react'
import { Alert, Button, Card, Input, InputNumber, Select, Switch, Table, Tabs, Tag, type TableColumn } from '@acme/components'
import DnsConfigEditor from '../features/subscriptions/toolbox/DnsConfigEditor'
import { api } from '../lib/api'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import type { SubscriptionConfigurationContext, SubscriptionEditorConfig } from '../lib/types'

interface DnsRewrite {
  id: string
  enabled: boolean
  domain: string
  type: string
  answer: string
  ttl: number
  comment: string
}

interface DnsSettings {
  schema: number
  revision: number
  use_system_dns: boolean
  config: string
  dns: unknown
  rewrites: DnsRewrite[]
  query_log_enabled: boolean
  query_log_max_entries: number
}

interface DnsFrontendStatus {
  enabled: boolean
  running: boolean
  core_dns_healthy: boolean
  mode: string
  core_upstream: string
  original_upstreams: string[]
  domestic_domain_source: string
  domestic_domain_count: number
  last_error?: string
}

interface DnsSettingsResponse {
  settings: DnsSettings
  status: DnsFrontendStatus
  editor_defaults: SubscriptionEditorConfig & { by_core?: Record<string, SubscriptionEditorConfig> }
  configuration_context?: SubscriptionConfigurationContext
}

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
  const context = settings.data?.configuration_context
  const defaults = settings.data?.editor_defaults
  const recommended = defaults?.by_core?.[context?.target?.core ?? ''] ?? defaults
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
    { title: zh ? '时间' : 'Time', dataIndex: 'time', width: 180, render: (value) => new Date(Number(value)).toLocaleString() },
    { title: zh ? '客户端' : 'Client', dataIndex: 'client', width: 130 },
    { title: zh ? '域名' : 'Name', dataIndex: 'name', minWidth: 220 },
    { title: zh ? '类型' : 'Type', dataIndex: 'type', width: 80 },
    { title: zh ? '决策' : 'Decision', dataIndex: 'decision', width: 100, render: (value) => <Tag color={value === 'local' ? 'green' : value === 'rewrite' ? 'blue' : value === 'reject' || value === 'error' ? 'red' : 'orange'}>{String(value)}</Tag> },
    { title: zh ? '应答' : 'Answers', dataIndex: 'answers', minWidth: 260, render: (value) => (value as string[]).join(', ') || '-' },
    { title: zh ? '上游' : 'Upstream', dataIndex: 'upstream', minWidth: 170 },
    { title: zh ? '耗时' : 'Latency', dataIndex: 'latency_ms', width: 90, render: (value) => `${value} ms` },
    { title: '', key: 'action', width: 60, render: (_value, item) => <Button size="small" variant="text" title={zh ? '添加重写' : 'Add rewrite'} onClick={() => setRewrite({ ...emptyRewrite(), domain: item.name.replace(/\.$/, ''), type: item.type === 'AAAA' ? 'AAAA' : 'A' })}><Plus size={14} /></Button> },
  ], [zh])
  const rewriteColumns = useMemo<Array<TableColumn<DnsRewrite>>>(() => [
    { title: zh ? '启用' : 'Enabled', dataIndex: 'enabled', width: 80, render: (value, item) => <Switch size="small" checked={Boolean(value)} onChange={(enabled) => current && setDraft({ ...current, rewrites: current.rewrites.map((rule) => rule.id === item.id ? { ...rule, enabled } : rule) })} /> },
    { title: zh ? '域名' : 'Domain', dataIndex: 'domain', minWidth: 220 },
    { title: zh ? '类型' : 'Type', dataIndex: 'type', width: 90 },
    { title: zh ? '应答' : 'Answer', dataIndex: 'answer', minWidth: 220 },
    { title: 'TTL', dataIndex: 'ttl', width: 90 },
    { title: zh ? '备注' : 'Comment', dataIndex: 'comment', minWidth: 160 },
    { title: '', key: 'action', width: 60, render: (_value, item) => <Button size="small" variant="text" title={zh ? '删除' : 'Delete'} onClick={() => current && setDraft({ ...current, rewrites: current.rewrites.filter((rule) => rule.id !== item.id) })}><Trash2 size={14} /></Button> },
  ], [current, zh])
  if (!current) return <div className="p-8 text-sm text-[var(--muted)]">{zh ? '正在加载 DNS 设置…' : 'Loading DNS settings…'}</div>
  const status = settings.data?.status
  const tabs = [
    { key: 'queries', label: zh ? '查询日志' : 'Query log', children: <QueryLog filter={filter} setFilter={setFilter} rows={visibleQueries} columns={queryColumns} clear={() => clear.mutate()} exporting={() => exportQueries(visibleQueries)} zh={zh} /> },
    { key: 'rewrites', label: zh ? 'DNS 重写' : 'DNS rewrites', children: <Rewrites current={current} rewrite={rewrite} setRewrite={setRewrite} add={addRewrite} columns={rewriteColumns} zh={zh} /> },
    { key: 'settings', label: zh ? '设置与状态' : 'Settings & status', children: <Settings current={current} setDraft={setDraft} status={status} features={context?.capabilities.features ?? []} recommended={recommended?.dns_config ?? ''} zh={zh} /> },
  ]
  return <div className="space-y-5">
    <div className="flex min-h-10 items-start justify-between gap-4">
      <div><h1 className="text-xl font-semibold">DNS</h1><p className="mt-1 text-sm text-[var(--muted)]">{zh ? '设备级 DNS 策略；切换订阅和路由规则不会改变这里的配置。' : 'Device-level DNS policy. Switching subscriptions or routing rules does not change it.'}</p></div>
      <div className="flex gap-2"><Button icon={<RefreshCw size={16} />} onClick={() => { settings.refetch(); queries.refetch() }}>{zh ? '刷新' : 'Refresh'}</Button><Button variant="primary" icon={<Save size={16} />} loading={save.isPending} onClick={() => save.mutate(current)}>{zh ? '保存并暂存' : 'Save & stage'}</Button></div>
    </div>
    {save.isError ? <Alert type="error" showIcon message={save.error instanceof Error ? save.error.message : String(save.error)} /> : null}
    {save.isSuccess ? <Alert type="success" showIcon message={zh ? 'DNS 设置已保存、重新编译并暂存。' : 'DNS settings were saved, recompiled, and staged.'} /> : null}
    <Card className="!rounded-lg" bodyStyle={{ padding: '1rem' }}><Tabs items={tabs} defaultActiveKey="queries" destroyInactiveTabPane={false} /></Card>
  </div>
}

function QueryLog({ filter, setFilter, rows, columns, clear, exporting, zh }: { filter: string; setFilter: (value: string) => void; rows: DnsQueryEvent[]; columns: Array<TableColumn<DnsQueryEvent>>; clear: () => void; exporting: () => void; zh: boolean }) {
  return <div className="space-y-3 pt-4"><div className="flex flex-wrap gap-2"><Input className="min-w-64 flex-1" value={filter} placeholder={zh ? '筛选域名、客户端、应答或原因' : 'Filter name, client, answer, or reason'} onChange={(event) => setFilter(event.target.value)} /><Button icon={<Download size={15} />} onClick={exporting}>{zh ? '导出' : 'Export'}</Button><Button danger icon={<Trash2 size={15} />} onClick={clear}>{zh ? '清空' : 'Clear'}</Button></div><Table<DnsQueryEvent> rowKey={(item) => `${item.time}-${item.client}-${item.name}`} size="middle" pagination={{ pageSize: 50 }} columns={columns} dataSource={rows} scroll={{ x: 1300 }} locale={{ emptyText: zh ? '暂无 DNS 查询' : 'No DNS queries' }} /></div>
}

function Rewrites({ current, rewrite, setRewrite, add, columns, zh }: { current: DnsSettings; rewrite: DnsRewrite; setRewrite: (value: DnsRewrite) => void; add: () => void; columns: Array<TableColumn<DnsRewrite>>; zh: boolean }) {
  return <div className="space-y-4 pt-4"><div className="grid gap-2 rounded-md border border-[var(--border)] p-3 md:grid-cols-6"><Input value={rewrite.domain} placeholder={zh ? '域名或 *.example.com' : 'Domain or *.example.com'} onChange={(event) => setRewrite({ ...rewrite, domain: event.target.value })} /><Select value={rewrite.type} options={['A', 'AAAA', 'CNAME'].map((value) => ({ value, label: value }))} onChange={(type) => setRewrite({ ...rewrite, type })} /><Input className="md:col-span-2" value={rewrite.answer} placeholder={zh ? 'IP 或目标域名' : 'IP or target name'} onChange={(event) => setRewrite({ ...rewrite, answer: event.target.value })} /><InputNumber className="w-full" min={0} value={rewrite.ttl} onChange={(ttl) => setRewrite({ ...rewrite, ttl: ttl ?? 300 })} /><Button variant="primary" icon={<Plus size={15} />} onClick={add}>{zh ? '添加' : 'Add'}</Button><Input className="md:col-span-6" value={rewrite.comment} placeholder={zh ? '备注（可选）' : 'Comment (optional)'} onChange={(event) => setRewrite({ ...rewrite, comment: event.target.value })} /></div><Table<DnsRewrite> rowKey="id" size="middle" pagination={false} columns={columns} dataSource={current.rewrites} scroll={{ x: 900 }} locale={{ emptyText: zh ? '暂无 DNS 重写' : 'No DNS rewrites' }} /></div>
}

function Settings({ current, setDraft, status, features, recommended, zh }: { current: DnsSettings; setDraft: (value: DnsSettings) => void; status?: DnsFrontendStatus; features: string[]; recommended: string; zh: boolean }) {
  return <div className="space-y-5 pt-4"><div className="grid gap-3 md:grid-cols-4"><Metric label={zh ? '前置 DNS' : 'DNS frontend'} value={status?.running ? (zh ? '运行中' : 'Running') : status?.enabled ? (zh ? '待启动' : 'Pending') : (zh ? '未启用' : 'Disabled')} /><Metric label={zh ? '模式' : 'Mode'} value={status?.mode || '-'} /><Metric label={zh ? '核心 DNS' : 'Core DNS'} value={status?.core_dns_healthy ? (zh ? '正常' : 'Healthy') : '-'} /><Metric label={zh ? '国内域名规则' : 'Domestic domains'} value={String(status?.domestic_domain_count ?? 0)} /></div>{status?.last_error ? <Alert type="error" showIcon message={status.last_error} /> : null}<div className="flex items-center justify-between rounded-md border border-[var(--border)] p-3"><div><div className="text-sm font-medium">{zh ? '使用核心推荐 DNS' : 'Use core-recommended DNS'}</div><div className="mt-1 text-xs text-[var(--muted)]">{zh ? '关闭后使用下面的设备级自定义 DNS。' : 'Turn off to use the device-level custom DNS below.'}</div></div><Switch checked={current.use_system_dns} onChange={(use_system_dns) => setDraft({ ...current, use_system_dns })} /></div><DnsConfigEditor value={current.use_system_dns ? recommended : current.config} readOnly={current.use_system_dns} features={features} onChange={(config) => setDraft({ ...current, config })} /><div className="grid gap-3 md:grid-cols-2"><label className="flex items-center justify-between rounded-md border border-[var(--border)] p-3 text-sm">{zh ? '记录 DNS 查询' : 'Record DNS queries'}<Switch checked={current.query_log_enabled} onChange={(query_log_enabled) => setDraft({ ...current, query_log_enabled })} /></label><label className="flex items-center gap-3 rounded-md border border-[var(--border)] p-3 text-sm"><span className="flex-1">{zh ? '最多保留条数' : 'Maximum entries'}</span><InputNumber min={100} max={20000} value={current.query_log_max_entries} onChange={(query_log_max_entries) => setDraft({ ...current, query_log_max_entries: query_log_max_entries ?? 2000 })} /></label></div></div>
}

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
