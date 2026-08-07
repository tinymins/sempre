import { useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Copy, Network, Play, RefreshCw, Save, Search, ShieldCheck, Terminal, Trash2 } from 'lucide-react'
import { Alert, Button, Card, Input, InputNumber, Select, Switch, Table, Tag, TextArea, type TableColumn } from '@acme/components'
import { api } from '../lib/api'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import type { GatewayConfig, GatewayDNSDebugResult, GatewayHostPlan, GatewayLease, GatewayStatus } from '../lib/types'

const emptyStatus: GatewayStatus = {
  config: {
    schema: 1,
    topology: 'local-pve',
    lan: { interface: '', gateway_cidr: '10.10.10.1/24', wan_interface: '', nat_enabled: false },
    dhcp: { enabled: false, range_start: '10.10.10.100', range_end: '10.10.10.200', lease_time: '12h', reservations: [] },
    dns: { enabled: false, listen_hosts: ['10.10.10.1'], listen_port: 53, local_upstreams: ['223.5.5.5:53', '223.6.6.6:53'], remote_upstream: '127.0.0.1:1053', strategy: 'local-first-classify', reject_https: true, rule_sets: [], domestic_cidrs: [], cache_ttl_seconds: 300 },
    pve: { port: 22, user: 'root', apply_persistent: false },
  },
  runtime: { dns_running: false, dhcp_running: false, dhcp_leases: [] },
  inventory: { supported: false, recommended_lan_interfaces: [], local_prefixes: [], vpn_prefixes: [], occupied_prefixes: [], interfaces: [] },
  validation_errors: [],
  host_plan_available: true,
}

export function Gateway() {
  const { t } = useI18n()
  const { session } = useSession()
  const queryClient = useQueryClient()
  const [draft, setDraft] = useState<GatewayConfig | null>(null)
  const [plan, setPlan] = useState<GatewayHostPlan | null>(null)
  const [dnsName, setDNSName] = useState('www.baidu.com')
  const [dnsResult, setDNSResult] = useState<GatewayDNSDebugResult | null>(null)
  const [sshKey, setSSHKey] = useState('')
  const status = useQuery({
    queryKey: ['gateway'],
    queryFn: () => api<GatewayStatus>(session!, '/gateway'),
    enabled: Boolean(session),
    refetchInterval: 5000,
  })
  const config = draft ?? status.data?.config ?? emptyStatus.config
  const inventory = status.data?.inventory ?? emptyStatus.inventory
  const runtime = status.data?.runtime ?? emptyStatus.runtime
  const validation = status.data?.validation_errors ?? []
  const lanOptions = useMemo(() => inventory.interfaces.map((item) => ({ value: item.name, label: `${item.name}${item.addresses.length ? ` · ${item.addresses.join(', ')}` : ''}` })), [inventory.interfaces])
  const save = useMutation({
    mutationFn: (next: GatewayConfig) => api<{ config: GatewayConfig; reload_requested: boolean }>(session!, '/gateway', { method: 'PUT', body: JSON.stringify(next) }),
    onSuccess: (result) => {
      setDraft(result.config)
      queryClient.invalidateQueries({ queryKey: ['gateway'] })
    },
  })
  const buildPlan = useMutation({
    mutationFn: (next: GatewayConfig) => api<GatewayHostPlan>(session!, '/gateway/host-plan', { method: 'POST', body: JSON.stringify({ config: next }) }),
    onSuccess: setPlan,
  })
  const applyPlan = useMutation({
    mutationFn: (next: GatewayConfig) => api<GatewayHostPlan>(session!, '/gateway/host-apply', { method: 'POST', body: JSON.stringify({ config: next, confirm: true, private_key: sshKey }) }),
    onSuccess: setPlan,
  })
  const queryDNS = useMutation({
    mutationFn: () => api<GatewayDNSDebugResult>(session!, '/gateway/dns/query', { method: 'POST', body: JSON.stringify({ name: dnsName, type: 'A' }) }),
    onSuccess: setDNSResult,
  })
  const revokeLease = useMutation({
    mutationFn: (mac: string) => api(session!, '/gateway/dhcp/leases/revoke', { method: 'POST', body: JSON.stringify({ mac }) }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['gateway'] }),
  })
  const update = (change: (current: GatewayConfig) => GatewayConfig) => setDraft(change(config))
  const leaseColumns = useMemo<Array<TableColumn<GatewayLease>>>(() => [
    { title: 'MAC', dataIndex: 'mac', minWidth: 180 },
    { title: 'IP', dataIndex: 'ip', width: 150 },
    { title: 'Hostname', dataIndex: 'hostname', minWidth: 160, render: (value) => value || '-' },
    { title: 'Type', key: 'type', width: 110, render: (_value, record) => <Tag color={record.reserved ? 'blue' : 'green'}>{record.reserved ? 'Reserved' : 'Dynamic'}</Tag> },
    { title: 'Expires', dataIndex: 'expires_at', minWidth: 180, render: (value) => value ? new Date(String(value)).toLocaleString() : '-' },
    { title: '', key: 'action', width: 70, render: (_value, record) => record.reserved ? null : <Button size="small" variant="text" title="Revoke" onClick={() => revokeLease.mutate(record.mac)}><Trash2 size={15} /></Button> },
  ], [revokeLease])

  return <div className="space-y-5">
    <div className="flex min-h-10 items-start justify-between gap-4">
      <div><h1 className="text-xl font-semibold">{t('gateway')}</h1><p className="mt-1 text-sm text-[var(--muted)]">DHCP, DNS, PVE host preparation, and LAN transparent gateway controls.</p></div>
      <div className="flex gap-2">
        <Button icon={<RefreshCw size={16} />} disabled={status.isFetching} onClick={() => status.refetch()}>{t('refresh')}</Button>
        <Button variant="primary" icon={<Save size={16} />} loading={save.isPending} onClick={() => save.mutate(config)}>{t('save')}</Button>
      </div>
    </div>

    {validation.length ? <Alert type="warning" showIcon message="Configuration needs attention" description={validation.join('; ')} /> : null}
    {save.isError ? <Alert type="error" showIcon message={save.error instanceof Error ? save.error.message : t('operationFailed')} /> : null}
    {save.isSuccess ? <Alert type="success" showIcon message={t('operationDone')} description="Saved. Running services will reload through the managed runtime." /> : null}

    <div className="grid gap-3 md:grid-cols-4">
      <Metric icon={Network} label="Topology" value={config.topology === 'local-pve' ? 'Local PVE' : 'Remote PVE'} tone="blue" />
      <Metric icon={ShieldCheck} label="DNS" value={runtime.dns_running ? 'Running' : config.dns.enabled ? 'Pending' : 'Disabled'} tone="green" />
      <Metric icon={Play} label="DHCP" value={runtime.dhcp_running ? 'Running' : config.dhcp.enabled ? 'Pending' : 'Disabled'} tone="amber" />
      <Metric icon={Terminal} label="Host plan" value={plan ? 'Generated' : 'Ready'} tone="cyan" />
    </div>

    <Card className="!rounded-lg" bodyStyle={{ padding: '1rem' }}>
      <Section title="Topology and LAN">
        <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-4">
          <Field label="Topology"><Select value={config.topology} options={[{ value: 'local-pve', label: 'PVE host local' }, { value: 'remote-pve', label: 'Gateway VM/LXC + PVE SSH/manual' }]} onChange={(value) => update((current) => ({ ...current, topology: value }))} /></Field>
          <Field label="LAN interface"><Select showSearch allowClear value={config.lan.interface} options={lanOptions} onChange={(value) => update((current) => ({ ...current, lan: { ...current.lan, interface: value || '' } }))} /></Field>
          <Field label="Gateway CIDR"><Input value={config.lan.gateway_cidr} onChange={(event) => update((current) => ({ ...current, lan: { ...current.lan, gateway_cidr: event.target.value } }))} /></Field>
          <Field label="WAN interface"><Input value={config.lan.wan_interface} onChange={(event) => update((current) => ({ ...current, lan: { ...current.lan, wan_interface: event.target.value } }))} /></Field>
          <Field label="NAT masquerade"><Switch checked={config.lan.nat_enabled} onChange={(value) => update((current) => ({ ...current, lan: { ...current.lan, nat_enabled: value } }))} /></Field>
          <Field label="PVE host"><Input value={config.pve.host || ''} disabled={config.topology === 'local-pve'} onChange={(event) => update((current) => ({ ...current, pve: { ...current.pve, host: event.target.value } }))} /></Field>
          <Field label="SSH user"><Input value={config.pve.user || 'root'} disabled={config.topology === 'local-pve'} onChange={(event) => update((current) => ({ ...current, pve: { ...current.pve, user: event.target.value } }))} /></Field>
          <Field label="SSH port"><InputNumber className="w-full" min={1} max={65535} value={config.pve.port || 22} disabled={config.topology === 'local-pve'} onChange={(value) => update((current) => ({ ...current, pve: { ...current.pve, port: value ?? 22 } }))} /></Field>
          <Field label="SSH key path"><Input value={config.pve.key_path || ''} disabled={config.topology === 'local-pve'} onChange={(event) => update((current) => ({ ...current, pve: { ...current.pve, key_path: event.target.value } }))} /></Field>
          <Field label="Host fingerprint"><Input value={config.pve.fingerprint || ''} disabled={config.topology === 'local-pve'} onChange={(event) => update((current) => ({ ...current, pve: { ...current.pve, fingerprint: event.target.value } }))} /></Field>
          <Field label="Persistent apply"><Switch checked={config.pve.apply_persistent} onChange={(value) => update((current) => ({ ...current, pve: { ...current.pve, apply_persistent: value } }))} /></Field>
        </div>
      </Section>
    </Card>

    <div className="grid gap-5 xl:grid-cols-2">
      <Card className="!rounded-lg" bodyStyle={{ padding: '1rem' }}>
        <Section title="DHCP">
          <div className="grid gap-3 md:grid-cols-2">
            <Field label="Enabled"><Switch checked={config.dhcp.enabled} onChange={(value) => update((current) => ({ ...current, dhcp: { ...current.dhcp, enabled: value } }))} /></Field>
            <Field label="Lease time"><Input value={config.dhcp.lease_time} onChange={(event) => update((current) => ({ ...current, dhcp: { ...current.dhcp, lease_time: event.target.value } }))} /></Field>
            <Field label="Range start"><Input value={config.dhcp.range_start} onChange={(event) => update((current) => ({ ...current, dhcp: { ...current.dhcp, range_start: event.target.value } }))} /></Field>
            <Field label="Range end"><Input value={config.dhcp.range_end} onChange={(event) => update((current) => ({ ...current, dhcp: { ...current.dhcp, range_end: event.target.value } }))} /></Field>
            <Field label="Domain"><Input value={config.dhcp.domain || ''} onChange={(event) => update((current) => ({ ...current, dhcp: { ...current.dhcp, domain: event.target.value } }))} /></Field>
          </div>
        </Section>
      </Card>

      <Card className="!rounded-lg" bodyStyle={{ padding: '1rem' }}>
        <Section title="DNS split">
          <div className="grid gap-3 md:grid-cols-2">
            <Field label="Enabled"><Switch checked={config.dns.enabled} onChange={(value) => update((current) => ({ ...current, dns: { ...current.dns, enabled: value } }))} /></Field>
            <Field label="Strategy"><Select value={config.dns.strategy} options={[{ value: 'local-first-classify', label: 'Local first classify' }, { value: 'rules-first', label: 'Rules first' }]} onChange={(value) => update((current) => ({ ...current, dns: { ...current.dns, strategy: value } }))} /></Field>
            <Field label="Listen hosts"><Input value={config.dns.listen_hosts.join(', ')} onChange={(event) => update((current) => ({ ...current, dns: { ...current.dns, listen_hosts: splitList(event.target.value) } }))} /></Field>
            <Field label="Listen port"><InputNumber className="w-full" min={1} max={65535} value={config.dns.listen_port} onChange={(value) => update((current) => ({ ...current, dns: { ...current.dns, listen_port: value ?? 53 } }))} /></Field>
            <Field label="Local upstreams"><Input value={config.dns.local_upstreams.join(', ')} onChange={(event) => update((current) => ({ ...current, dns: { ...current.dns, local_upstreams: splitList(event.target.value) } }))} /></Field>
            <Field label="Remote upstream"><Input value={config.dns.remote_upstream} onChange={(event) => update((current) => ({ ...current, dns: { ...current.dns, remote_upstream: event.target.value } }))} /></Field>
            <Field label="Reject HTTPS"><Switch checked={config.dns.reject_https} onChange={(value) => update((current) => ({ ...current, dns: { ...current.dns, reject_https: value } }))} /></Field>
          </div>
          <div className="mt-4 flex gap-2">
            <Input value={dnsName} onChange={(event) => setDNSName(event.target.value)} />
            <Button icon={<Search size={16} />} loading={queryDNS.isPending} onClick={() => queryDNS.mutate()}>Query</Button>
          </div>
          {dnsResult ? <div className="mt-3 rounded-md border border-[var(--border)] bg-[var(--surface)] p-3 text-xs"><div>Upstream: {dnsResult.upstream}</div><div className="mt-1 whitespace-pre-wrap">{dnsResult.answers.join('\n') || 'No answers'}</div></div> : null}
        </Section>
      </Card>
    </div>

    <Card className="!rounded-lg" bodyStyle={{ padding: '1rem' }}>
      <Section title="PVE host preparation">
        {config.topology === 'remote-pve' ? <div className="mb-3"><Field label="One-time SSH private key"><TextArea rows={3} value={sshKey} placeholder="Optional when SSH key path is configured on the Sempre host" onChange={(event) => setSSHKey(event.target.value)} /></Field></div> : null}
        <div className="mb-3 flex gap-2">
          <Button icon={<Terminal size={16} />} loading={buildPlan.isPending} onClick={() => buildPlan.mutate(config)}>Generate commands</Button>
          <Button icon={<Copy size={16} />} disabled={!plan} onClick={() => plan && navigator.clipboard.writeText([...plan.commands, ...plan.persistent_commands].join('\n'))}>Copy</Button>
          <Button variant="primary" icon={<Play size={16} />} loading={applyPlan.isPending} onClick={() => window.confirm('Apply these commands to the host now?') && applyPlan.mutate(config)}>Apply confirmed plan</Button>
        </div>
        {buildPlan.isError ? <Alert type="error" showIcon message={buildPlan.error instanceof Error ? buildPlan.error.message : t('operationFailed')} /> : null}
        {applyPlan.isError ? <Alert type="error" showIcon message={applyPlan.error instanceof Error ? applyPlan.error.message : t('operationFailed')} /> : null}
        {plan ? <div className="space-y-3">
          <Alert type="info" showIcon message={plan.summary} description={plan.warnings.join(' ')} />
          <TextArea rows={Math.min(12, Math.max(4, plan.commands.length + plan.persistent_commands.length + 1))} value={[...plan.commands, ...plan.persistent_commands].join('\n')} readOnly />
          {plan.output?.length ? <TextArea rows={Math.min(10, Math.max(3, plan.output.length + 1))} value={plan.output.join('\n')} readOnly /> : null}
        </div> : null}
      </Section>
    </Card>

    <Card className="!rounded-lg" bodyStyle={{ padding: 0 }}>
      <div className="border-b border-[var(--border)] px-4 py-3"><h2 className="text-sm font-semibold">DHCP leases</h2></div>
      <Table<GatewayLease> rowKey="mac" size="middle" pagination={false} columns={leaseColumns} dataSource={runtime.dhcp_leases} scroll={{ x: 900 }} locale={{ emptyText: 'No DHCP leases' }} />
    </Card>
  </div>
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return <section className="space-y-3"><h2 className="text-sm font-semibold">{title}</h2>{children}</section>
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return <label className="block min-w-0 space-y-1"><span className="block text-xs font-medium text-[var(--muted)]">{label}</span>{children}</label>
}

function Metric({ icon: Icon, label, value, tone }: { icon: typeof Network; label: string; value: string; tone: 'green' | 'amber' | 'cyan' | 'blue' }) {
  const colors = {
    green: 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400',
    amber: 'bg-amber-500/12 text-amber-700 dark:text-amber-400',
    cyan: 'bg-cyan-500/10 text-cyan-700 dark:text-cyan-400',
    blue: 'bg-blue-500/10 text-blue-700 dark:text-blue-400',
  }
  return <Card className="!rounded-lg" bodyStyle={{ padding: '1rem' }}><span className={`grid size-8 place-items-center rounded-md ${colors[tone]}`}><Icon size={17} /></span><p className="mt-5 truncate text-xs text-[var(--muted)]">{label}</p><p className="mt-1 truncate text-lg font-semibold tabular-nums" title={value}>{value}</p></Card>
}

function splitList(value: string) {
  return value.split(',').map((item) => item.trim()).filter(Boolean)
}
