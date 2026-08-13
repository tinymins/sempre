import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { CirclePlus, Download, FileText, Play, RefreshCw, RotateCw, Save, Square, Trash2 } from 'lucide-react'
import { Alert, Button, Card, Collapse, Input, InputNumber, Switch, TextArea } from '@acme/components'
import { api } from '../lib/api'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import type { TunnelConfig, TunnelForward, TunnelInstance, TunnelStatus } from '../lib/types'

const emptyStatus: TunnelStatus = {
  config: { schema: 1, instances: [] },
  binary: { version: '10.5.5', installed: false },
  instances: [],
  forwards: [],
}

export function Tunnels() {
  const { session } = useSession()
  const { locale } = useI18n()
  const copy = locale === 'zh-CN' ? zh : en
  const queryClient = useQueryClient()
  const [draft, setDraft] = useState<TunnelConfig | null>(null)
  const [log, setLog] = useState<{ name: string; content: string } | null>(null)
  const status = useQuery({ queryKey: ['tunnels'], queryFn: () => api<TunnelStatus>(session!, '/tunnels'), enabled: Boolean(session), refetchInterval: 3000 })
  const config = draft ?? status.data?.config ?? emptyStatus.config
  const dirty = JSON.stringify(config) !== JSON.stringify(status.data?.config ?? emptyStatus.config)
  const save = useMutation({
    mutationFn: (next: TunnelConfig) => api<{ status: TunnelStatus }>(session!, '/tunnels', { method: 'PUT', body: JSON.stringify(next) }),
    onSuccess: (result) => { setDraft(null); queryClient.setQueryData(['tunnels'], result.status) },
  })
  const install = useMutation({
    mutationFn: () => api(session!, '/tunnels/install', { method: 'POST' }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['tunnels'] }),
  })
  const action = useMutation({
    mutationFn: ({ id, value }: { id: string; value: string }) => api<{ status: TunnelStatus }>(session!, `/tunnels/${encodeURIComponent(id)}/${value}`, { method: 'POST' }),
    onSuccess: (result) => { setDraft(null); queryClient.setQueryData(['tunnels'], result.status) },
  })
  const loadLog = useMutation({
    mutationFn: async (instance: TunnelInstance) => ({ name: instance.name, ...(await api<{ content: string }>(session!, `/tunnels/${encodeURIComponent(instance.id)}/log`)) }),
    onSuccess: setLog,
  })
  const update = (next: TunnelConfig) => setDraft(next)
  const runtimeByID = new Map((status.data?.instances ?? []).map((item) => [item.id, item]))

  return <div className="space-y-5">
    <div className="flex min-h-10 flex-wrap items-start justify-between gap-3">
      <div><h1 className="text-xl font-semibold">{copy.title}</h1><p className="mt-1 text-sm text-[var(--muted)]">{copy.detail}</p></div>
      <div className="flex gap-2"><Button icon={<RefreshCw size={16} />} onClick={() => status.refetch()}>{copy.refresh}</Button><Button variant="primary" icon={<Save size={16} />} loading={save.isPending} onClick={() => save.mutate(config)}>{copy.save}</Button></div>
    </div>
    <Alert type="info" showIcon message={copy.safetyTitle} description={copy.safetyDetail} />
    {(status.data?.binary ?? emptyStatus.binary).installed ? null : <Alert type="warning" showIcon message={`wstunnel ${status.data?.binary.version ?? emptyStatus.binary.version} ${copy.notInstalled}`} description={<Button className="mt-2" size="small" icon={<Download size={14} />} loading={install.isPending} onClick={() => install.mutate()}>{copy.download}</Button>} />}
    {status.isError || save.isError || install.isError || action.isError || loadLog.isError ? <Alert type="error" showIcon message={(status.error ?? save.error ?? install.error ?? action.error ?? loadLog.error) instanceof Error ? String((status.error ?? save.error ?? install.error ?? action.error ?? loadLog.error)?.message) : copy.failed} /> : null}
    {dirty ? <Alert type="warning" showIcon message={copy.unsaved} /> : null}
    <div className="flex justify-end"><Button icon={<CirclePlus size={16} />} onClick={() => update({ ...config, instances: [...config.instances, newInstance()] })}>{copy.addInstance}</Button></div>
    {config.instances.length === 0 ? <Card className="p-8 text-center text-sm text-[var(--muted)]">{copy.empty}</Card> : config.instances.map((instance, index) => {
      const runtime = runtimeByID.get(instance.id)
      return <Card key={instance.id} className="!rounded-lg" bodyStyle={{ padding: '1rem' }}>
        <div className="flex flex-wrap items-center justify-between gap-2"><div><h2 className="font-semibold">{instance.name || `${copy.instance} ${index + 1}`}</h2><p className="mt-1 text-xs text-[var(--muted)]">{stateLabel(copy, runtime?.state ?? 'stopped')} · {copy.restarts} {runtime?.restart_count ?? 0}</p></div><div className="flex gap-1"><Button size="small" icon={<Play size={14} />} disabled={dirty || instance.desired_state === 'running'} onClick={() => action.mutate({ id: instance.id, value: 'start' })}>{copy.start}</Button><Button size="small" icon={<Square size={14} />} disabled={dirty || instance.desired_state === 'stopped'} onClick={() => action.mutate({ id: instance.id, value: 'stop' })}>{copy.stop}</Button><Button size="small" icon={<RotateCw size={14} />} disabled={dirty || instance.desired_state === 'stopped'} onClick={() => action.mutate({ id: instance.id, value: 'restart' })}>{copy.restart}</Button><Button size="small" icon={<FileText size={14} />} disabled={dirty} onClick={() => loadLog.mutate(instance)}>{copy.log}</Button><Button size="small" variant="danger" icon={<Trash2 size={14} />} onClick={() => update({ ...config, instances: config.instances.filter((_, itemIndex) => itemIndex !== index) })}>{copy.remove}</Button></div></div>
        {runtime?.last_error ? <Alert className="mt-3" type="error" showIcon message={runtime.last_error} /> : null}
        <div className="mt-4 grid gap-3 md:grid-cols-2 xl:grid-cols-[1fr_2fr_auto]">
          <Field label={copy.name}><Input value={instance.name} onChange={(event) => updateInstance(config, index, { name: event.target.value }, update)} /></Field>
          <Field label={copy.serverURL}><Input value={instance.server_url} placeholder="wss://hz.example.com:443" onChange={(event) => updateInstance(config, index, { server_url: event.target.value }, update)} /></Field>
          <Field label={copy.desiredRunning}><Switch checked={instance.desired_state === 'running'} onChange={(checked) => updateInstance(config, index, { desired_state: checked ? 'running' : 'stopped' }, update)} /></Field>
        </div>
        <Collapse className="mt-4" size="small" items={[{ key: 'advanced', label: copy.advancedSettings, children: <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-4">
          <Field label="DNS resolver"><Input value={instance.dns_resolvers.join(', ')} placeholder={copy.resolverPlaceholder} onChange={(event) => updateInstance(config, index, { dns_resolvers: splitList(event.target.value) }, update)} /></Field>
          <Field label={copy.preferIPv4}><Switch checked={instance.prefer_ipv4} onChange={(checked) => updateInstance(config, index, { prefer_ipv4: checked }, update)} /></Field>
          <Field label="WebSocket ping"><Input value={instance.websocket_ping} onChange={(event) => updateInstance(config, index, { websocket_ping: event.target.value }, update)} /></Field>
          <Field label={copy.retryBackoff}><Input value={instance.connection_retry_max_backoff} onChange={(event) => updateInstance(config, index, { connection_retry_max_backoff: event.target.value }, update)} /></Field>
          <Field label="Upgrade path"><Input value={instance.upgrade_path_prefix || ''} placeholder="v1" onChange={(event) => updateInstance(config, index, { upgrade_path_prefix: event.target.value }, update)} /></Field>
        </div> }]} />
        <div className="mt-5 flex items-center justify-between"><h3 className="text-sm font-semibold">{copy.udpForwards}</h3><Button size="small" icon={<CirclePlus size={14} />} onClick={() => updateInstance(config, index, { forwards: [...instance.forwards, newForward(config)] }, update)}>{copy.addForward}</Button></div>
        <div className="mt-2 space-y-2">{instance.forwards.map((forward, forwardIndex) => <ForwardRow key={forward.id} forward={forward} copy={copy} onChange={(change) => updateInstance(config, index, { forwards: instance.forwards.map((item, itemIndex) => itemIndex === forwardIndex ? { ...item, ...change } : item) }, update)} onRemove={() => updateInstance(config, index, { forwards: instance.forwards.filter((_, itemIndex) => itemIndex !== forwardIndex) }, update)} />)}</div>
      </Card>
    })}
    {log ? <Card className="!rounded-lg" bodyStyle={{ padding: '1rem' }}><div className="mb-2 flex items-center justify-between"><h2 className="text-sm font-semibold">{log.name} {copy.log}</h2><Button size="small" onClick={() => setLog(null)}>{copy.close}</Button></div><TextArea rows={14} value={log.content || copy.noLog} readOnly /></Card> : null}
  </div>
}

function ForwardRow({ forward, copy, onChange, onRemove }: { forward: TunnelForward; copy: Record<keyof typeof en, string>; onChange: (change: Partial<TunnelForward>) => void; onRemove: () => void }) {
  return <div className="rounded-md border border-[var(--border)] p-3">
    <div className="grid items-end gap-2 md:grid-cols-2 xl:grid-cols-[1fr_140px_140px_auto]">
      <Field label={copy.name}><Input value={forward.name} onChange={(event) => onChange({ name: event.target.value })} /></Field><Field label={copy.localPort}><InputNumber className="w-full" min={1} max={65535} value={forward.listen_port} onChange={(value) => onChange({ listen_port: value ?? 0 })} /></Field><Field label={copy.remotePort}><InputNumber className="w-full" min={1} max={65535} value={forward.remote_port} onChange={(value) => onChange({ remote_port: value ?? 0 })} /></Field><Button size="small" variant="danger" icon={<Trash2 size={14} />} onClick={onRemove}>{copy.remove}</Button>
    </div>
    <Collapse className="mt-3" size="small" items={[{ key: 'advanced', label: copy.advancedSettings, children: <div className="grid gap-3 md:grid-cols-2">
      <Field label={copy.remoteHost}><Input value={forward.remote_host} onChange={(event) => onChange({ remote_host: event.target.value })} /></Field><Field label={copy.timeout}><InputNumber className="w-full" min={0} value={forward.timeout_seconds} onChange={(value) => onChange({ timeout_seconds: value ?? 0 })} /></Field>
    </div> }]} />
  </div>
}

function Field({ label, children }: { label: string; children: React.ReactNode }) { return <label className="block min-w-0 space-y-1"><span className="block text-xs font-medium text-[var(--muted)]">{label}</span>{children}</label> }
function splitList(value: string) { return value.split(/[\n,]/).map((item) => item.trim()).filter(Boolean) }
function shortID(prefix: string) { return `${prefix}-${crypto.randomUUID().slice(0, 8)}` }
function newInstance(): TunnelInstance { return { id: shortID('tunnel'), name: '', desired_state: 'stopped', server_url: '', dns_resolvers: [], prefer_ipv4: true, websocket_ping: '15s', connection_retry_max_backoff: '30s', forwards: [] } }
function newForward(config: TunnelConfig): TunnelForward { const used = new Set(config.instances.flatMap((instance) => instance.forwards.map((forward) => forward.listen_port))); let port = 52001; while (used.has(port)) port += 1; return { id: shortID('wg'), name: '', listen_port: port, remote_host: '127.0.0.1', remote_port: 31088, timeout_seconds: 0 } }
function updateInstance(config: TunnelConfig, index: number, change: Partial<TunnelInstance>, apply: (next: TunnelConfig) => void) { apply({ ...config, instances: config.instances.map((item, itemIndex) => itemIndex === index ? { ...item, ...change } : item) }) }
function stateLabel(copy: Record<keyof typeof en, string>, state: string) { return state === 'running' ? copy.stateRunning : state === 'starting' ? copy.stateStarting : state === 'installing' ? copy.stateInstalling : state === 'restarting' ? copy.stateRestarting : state === 'stopping' ? copy.stateStopping : copy.stateStopped }

const zh = { title: '隧道', detail: '管理本机 wstunnel 客户端。Sempre 不会连接或配置远端 OpenWrt。', refresh: '刷新', save: '保存', safetyTitle: '安全边界', safetyDetail: '这里只启动本机客户端并监听 127.0.0.1。远端 wstunnel server、证书、防火墙和 OpenWrt 服务必须由你人工配置。', notInstalled: '尚未安装', download: '下载并校验', failed: '操作失败', unsaved: '请先保存当前修改，再执行启动、停止、重启或查看日志。', addInstance: '新增远端实例', empty: '尚未配置隧道。每台远端 OpenWrt 添加一个客户端实例。', instance: '实例', restarts: '重启次数', start: '启动', stop: '停止', restart: '重启', log: '日志', remove: '删除', name: '名称', serverURL: 'WSS 服务地址', desiredRunning: '期望运行', advancedSettings: '高级参数', preferIPv4: '优先 IPv4', retryBackoff: '最大重连退避', udpForwards: 'UDP 转发', addForward: '新增转发', close: '关闭', noLog: '暂无日志', localPort: '本地 UDP 端口', remoteHost: '远端主机', remotePort: '远端 UDP 端口', timeout: 'UDP 超时秒', resolverPlaceholder: 'dns://权威-NS-IP:53', stateRunning: '运行中', stateStarting: '启动中', stateInstalling: '安装中', stateRestarting: '重启中', stateStopping: '停止中', stateStopped: '已停止' }
const en = { title: 'Tunnels', detail: 'Manage local wstunnel clients. Sempre never connects to or configures remote OpenWrt hosts.', refresh: 'Refresh', save: 'Save', safetyTitle: 'Safety boundary', safetyDetail: 'Only local clients listening on 127.0.0.1 are managed here. Configure the remote wstunnel server, certificates, firewall, and OpenWrt service manually.', notInstalled: 'is not installed', download: 'Download and verify', failed: 'Operation failed', unsaved: 'Save the current changes before starting, stopping, restarting, or viewing logs.', addInstance: 'Add remote instance', empty: 'No tunnels configured. Add one client instance for each remote OpenWrt host.', instance: 'Instance', restarts: 'Restarts', start: 'Start', stop: 'Stop', restart: 'Restart', log: 'Log', remove: 'Remove', name: 'Name', serverURL: 'WSS server URL', desiredRunning: 'Desired running state', advancedSettings: 'Advanced settings', preferIPv4: 'Prefer IPv4', retryBackoff: 'Maximum retry backoff', udpForwards: 'UDP forwards', addForward: 'Add forward', close: 'Close', noLog: 'No log output', localPort: 'Local UDP port', remoteHost: 'Remote host', remotePort: 'Remote UDP port', timeout: 'UDP timeout (seconds)', resolverPlaceholder: 'dns://authoritative-NS-IP:53', stateRunning: 'Running', stateStarting: 'Starting', stateInstalling: 'Installing', stateRestarting: 'Restarting', stateStopping: 'Stopping', stateStopped: 'Stopped' }
