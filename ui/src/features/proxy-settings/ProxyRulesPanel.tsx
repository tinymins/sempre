import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Plus, Trash2 } from 'lucide-react'
import { Select, Table, useToast, type TableColumn } from '@acme/components'
import { Button, Field, Input } from '../../components/ui'
import { api } from '../../lib/api'
import { useI18n } from '../../lib/i18n'
import { useSession } from '../../lib/session'
import { fetchProxiesKey, type FetchProxySummary, type ResourceHostRule, type ResourceOutbound, type ResourceRules, type ResourceRulesTest } from './useFetchProxies'

const rulesKey = ['fetch-proxies', 'rules']
const value = (outbound: ResourceOutbound) => outbound.type === 'proxy' ? `proxy:${outbound.proxy_id}` : outbound.type
const parse = (text: string): ResourceOutbound => text.startsWith('proxy:') ? { type: 'proxy', proxy_id: text.slice(6) } : { type: text as 'system' | 'direct' }

export function ProxyRulesPanel({ proxies }: { proxies: FetchProxySummary[] }) {
  const { locale } = useI18n()
  const { session } = useSession()
  const toast = useToast()
  const queryClient = useQueryClient()
  const label = (zh: string, en: string) => locale === 'zh-CN' ? zh : en
  const rules = useQuery({ queryKey: rulesKey, queryFn: () => api<ResourceRules>(session!, '/fetch-proxies/rules') })
  const [drafts, setDrafts] = useState<Record<number, string>>({})
  const [blankRow, setBlankRow] = useState(false)
  const [blankHost, setBlankHost] = useState('')
  const [url, setUrl] = useState('https://raw.githubusercontent.com/dler-io/Rules/refs/heads/main/Clash/Provider/AdBlock.yaml')
  const save = useMutation({
    mutationFn: (next: ResourceRules) => api<ResourceRules>(session!, '/fetch-proxies/rules', { method: 'PUT', body: JSON.stringify(next) }),
    onSuccess: (saved) => {
      queryClient.setQueryData(rulesKey, saved)
      setDrafts({})
      return queryClient.invalidateQueries({ queryKey: fetchProxiesKey, exact: true })
    },
    onError: (error) => {
      toast.error(error.message)
      return queryClient.invalidateQueries({ queryKey: rulesKey })
    },
  })
  const test = useMutation({
    mutationFn: () => api<ResourceRulesTest>(session!, '/fetch-proxies/rules/test', { method: 'POST', body: JSON.stringify({ url: url.trim() }) }),
    onError: (error) => toast.error(error.message),
  })
  const current = rules.data
  const busy = save.isPending || test.isPending
  const outboundSelect = (outbound: ResourceOutbound, name: string, onChange: (outbound: ResourceOutbound) => void) => {
    const options = [
      { value: 'system', label: label('系统（默认路径）', 'System (default path)') },
      { value: 'direct', label: label('直连', 'Direct') },
      ...proxies.map((proxy) => ({ value: `proxy:${proxy.id}`, label: proxy.name, disabled: !proxy.enabled })),
    ]
    if (outbound.type === 'proxy' && !proxies.some((proxy) => proxy.id === outbound.proxy_id)) options.push({ value: value(outbound), label: label('代理不可用', 'Proxy unavailable'), disabled: true })
    return <Select aria-label={name} className="w-full min-w-0" disabled={busy} value={value(outbound)} options={options} onChange={(next) => onChange(parse(String(next)))} />
  }
  const changeRules = (patch: Partial<ResourceRules>) => { if (current) save.mutate({ ...current, ...patch }) }
  const changeRule = (index: number, patch: Partial<ResourceHostRule>) => changeRules({ host_rules: current!.host_rules.map((rule, position) => position === index ? { ...rule, ...patch } : rule) })
  const commitBlank = () => {
    const host = blankHost.trim()
    if (!host) return
    setBlankRow(false)
    setBlankHost('')
    changeRules({ host_rules: [...current!.host_rules, { host, outbound: { type: 'direct' } }] })
  }
  const columns: Array<TableColumn<ResourceHostRule & { index: number }>> = [
    { title: label('主机名', 'Host'), key: 'host', render: (_value, rule) => <Input aria-label={label('主机名', 'Host')} disabled={busy} value={drafts[rule.index] ?? rule.host} onChange={(event) => setDrafts((items) => ({ ...items, [rule.index]: event.target.value }))} onBlur={() => { const draft = drafts[rule.index]; if (draft !== undefined && draft.trim().toLowerCase() !== rule.host) changeRule(rule.index, { host: draft }) }} /> },
    { title: label('出口', 'Outbound'), key: 'outbound', width: 240, render: (_value, rule) => outboundSelect(rule.outbound, label('出口', 'Outbound'), (outbound) => changeRule(rule.index, { outbound })) },
    { title: '', key: 'actions', width: 60, render: (_value, rule) => <Button variant="ghost" title={label('删除', 'Delete')} aria-label={label('删除', 'Delete')} disabled={busy} onClick={() => changeRules({ host_rules: current!.host_rules.filter((_item, position) => position !== rule.index) })}><Trash2 size={15} /></Button> },
  ]
  if (!current) return <p className="p-4 text-sm text-[var(--muted)]">{rules.error?.message ?? label('正在加载…', 'Loading…')}</p>
  const result = test.data
  return <div className="space-y-6 pt-4">
    <p className="text-sm text-[var(--muted)]">{label('决定规则集、GeoIP、远程订阅、Sempre 更新、UI 和核心下载走哪个出口。选中的代理不可用时下载直接失败，不会改为直连。', 'Chooses the outbound for rule sets, GeoIP, remote subscriptions, Sempre updates, UI archives, and core downloads. If the chosen proxy is unavailable the download fails rather than connecting directly.')}</p>
    <Field label={label('默认出口', 'Default outbound')} hint={label('没有匹配规则时使用。', 'Used when no rule matches.')}><div className="max-w-sm">{outboundSelect(current.default_outbound, label('默认出口', 'Default outbound'), (default_outbound) => changeRules({ default_outbound }))}</div></Field>
    <section className="space-y-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div><h2 className="text-sm font-semibold">{label('按主机覆盖', 'Host overrides')}</h2><p className="mt-1 text-xs text-[var(--muted)]">{label('精确匹配主机名，不包含子域名。', 'Matches the exact hostname, not its subdomains.')}</p></div>
        <Button disabled={busy || blankRow} onClick={() => setBlankRow(true)}><Plus size={16} />{label('添加规则', 'Add rule')}</Button>
      </div>
      <Table<ResourceHostRule & { index: number }> size="small" rowKey="index" dataSource={current.host_rules.map((rule, index) => ({ ...rule, index }))} columns={columns} pagination={false} scroll={{ x: 560 }} />
      {blankRow ? <div className="flex gap-2"><Input autoFocus aria-label={label('新主机名', 'New host')} placeholder="raw.githubusercontent.com" value={blankHost} onChange={(event) => setBlankHost(event.target.value)} onBlur={commitBlank} /><Button variant="ghost" onClick={() => { setBlankRow(false); setBlankHost('') }}>{label('取消', 'Cancel')}</Button></div> : null}
    </section>
    <section className="space-y-3 border-t border-[var(--border)] pt-4">
      <div><h2 className="text-sm font-semibold">{label('测试规则', 'Test rules')}</h2><p className="mt-1 text-xs text-[var(--muted)]">{label('用已保存的规则请求一个地址。', 'Requests a URL with the saved rules.')}</p></div>
      <div className="flex flex-col gap-2 sm:flex-row"><Input className="min-w-0 flex-1" aria-label="URL" value={url} onChange={(event) => setUrl(event.target.value)} /><Button disabled={busy || !url.trim()} onClick={() => test.mutate()}>{test.isPending ? label('测试中…', 'Testing…') : label('测试', 'Test')}</Button></div>
      {result ? <dl role="status" className="grid grid-cols-[6rem_minmax(0,1fr)] gap-x-4 gap-y-2 text-sm">
        <dt className="text-[var(--muted)]">{label('主机名', 'Host')}</dt><dd className="min-w-0 break-all">{result.host}</dd>
        <dt className="text-[var(--muted)]">{label('命中规则', 'Matched')}</dt><dd className="min-w-0 break-all">{result.matched_host ?? label('默认出口', 'Default outbound')}</dd>
        <dt className="text-[var(--muted)]">{label('出口', 'Outbound')}</dt><dd className="min-w-0 break-all">{result.outbound_name}</dd>
        <dt className="text-[var(--muted)]">HTTP</dt><dd>{result.http_status ?? '—'}</dd>
        <dt className="text-[var(--muted)]">{label('耗时', 'Elapsed')}</dt><dd>{result.elapsed_ms} ms</dd>
        <dt className="text-[var(--muted)]">{label('大小', 'Size')}</dt><dd>{result.body_bytes} B</dd>
        <dt className="text-[var(--muted)]">{label('结果', 'Result')}</dt><dd>{result.ok ? label('成功', 'Success') : label('失败', 'Failed')}</dd>
        {result.error ? <><dt className="text-[var(--muted)]">{label('错误', 'Error')}</dt><dd className="min-w-0 break-all text-red-600">{result.error}</dd></> : null}
      </dl> : null}
    </section>
  </div>
}
