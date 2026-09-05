import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Button, Checkbox, Input, Switch, Tag } from '@acme/components'
import { Plus, Radar, Trash2 } from 'lucide-react'
import { api } from '../lib/api'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import type { KnownNetwork, NetworkSettings, NetworkSettingsResponse, SystemStatus } from '../lib/types'
import { Card } from './ui'

export function NetworkAutomationPanel() {
  const { locale } = useI18n()
  const { session } = useSession()
  const queryClient = useQueryClient()
  const zh = locale === 'zh-CN'
  const [drafts, setDrafts] = useState<Record<string, string>>({})
  const [notice, setNotice] = useState('')
  const network = useQuery({ queryKey: ['network', 'settings'], queryFn: () => api<NetworkSettingsResponse>(session!, '/network/settings') })
  const system = useQuery({ queryKey: ['system'], queryFn: () => api<SystemStatus>(session!, '/system'), refetchInterval: 5000 })
  const update = useMutation({
    mutationFn: (settings: NetworkSettings) => api<NetworkSettingsResponse>(session!, '/network/settings', { method: 'PUT', body: JSON.stringify(settings) }),
    onSuccess: (result) => {
      queryClient.setQueryData(['network', 'settings'], result)
      queryClient.invalidateQueries({ queryKey: ['system'] })
      setNotice(zh ? '已保存，配置变更需要应用后生效。' : 'Saved. Apply the pending configuration to activate it.')
    },
    onError: (error) => setNotice(error instanceof Error ? error.message : String(error)),
  })
  const settings = network.data?.settings
  const current = network.data?.current
  const status = system.data?.network_automation

  function save(patch: Partial<NetworkSettings>) {
    if (settings) update.mutate({ ...settings, ...patch })
  }

  function replaceNetwork(id: string, patch: Partial<KnownNetwork>) {
    if (!settings) return
    save({ known_networks: settings.known_networks.map((item) => item.id === id ? { ...item, ...patch } : item) })
  }

  function addCurrent() {
    if (!settings || !current?.gateway_mac) return
    if (settings.known_networks.some((item) => item.gateway_mac.toLowerCase() === current.gateway_mac?.toLowerCase())) {
      setNotice(zh ? '当前网络已经在列表中。' : 'The current network is already listed.')
      return
    }
    const suffix = current.gateway_mac.split(':').slice(-3).join(':')
    const item: KnownNetwork = {
      id: crypto.randomUUID(),
      name: zh ? `网络 ${suffix}` : `Network ${suffix}`,
      gateway_mac: current.gateway_mac,
      disable_proxy: true,
    }
    update.mutate({ ...settings, automatic_switching: true, known_networks: [...settings.known_networks, item] })
  }

  return <Card className="min-w-0 p-4 md:p-5">
    <div className="flex flex-wrap items-start justify-between gap-3">
      <div className="flex items-start gap-2"><Radar className="mt-0.5 text-emerald-600" size={18} /><div><h2 className="text-sm font-semibold">{zh ? '自动网络切换' : 'Automatic network switching'}</h2><p className="mt-1 text-xs leading-5 text-[var(--muted)]">{zh ? '进入已有透明代理的网络时，公网流量自动直连；WG 私网按各连接器选择的家庭网络单独判断。' : 'Public traffic goes direct on networks with an upstream transparent proxy. WireGuard home bypass is selected separately per connector.'}</p></div></div>
      <Switch checked={settings?.automatic_switching ?? false} loading={update.isPending} onChange={(checked) => save({ automatic_switching: checked })} />
    </div>

    <div className="mt-5 grid gap-3 md:grid-cols-[minmax(0,1fr)_auto] md:items-center">
      <div className="rounded-lg border border-[var(--border)] bg-[var(--surface-subtle)] px-3 py-2.5 text-xs">
        <div className="flex flex-wrap items-center gap-2"><span className="font-medium">{zh ? '当前网络' : 'Current network'}</span><Tag color={status?.path === 'direct' ? 'green' : status?.path === 'proxy' ? 'blue' : 'orange'}>{pathLabel(status?.path, zh)}</Tag>{status?.network_name ? <span>{status.network_name}</span> : null}</div>
        <p className="mt-1 break-all font-mono text-[var(--muted)]">{current?.name || '-'} · {current?.gateway || '-'} · {current?.gateway_mac || (zh ? '未获取到网关 MAC' : 'Gateway MAC unavailable')}</p>
      </div>
      <Button variant="primary" icon={<Plus size={16} />} loading={update.isPending} disabled={!current?.gateway_mac} onClick={addCurrent}>{zh ? '将当前网络加入' : 'Add current network'}</Button>
    </div>

    <div className="mt-5 space-y-2">
      {(settings?.known_networks ?? []).map((item) => <div key={item.id} className="grid gap-3 rounded-lg border border-[var(--border)] p-3 md:grid-cols-[minmax(10rem,1fr)_minmax(12rem,1fr)_auto_auto] md:items-center">
        <Input value={drafts[item.id] ?? item.name} onChange={(event) => setDrafts((value) => ({ ...value, [item.id]: event.target.value }))} onBlur={() => { const name = drafts[item.id]?.trim(); if (name && name !== item.name) replaceNetwork(item.id, { name }); setDrafts((value) => { const next = { ...value }; delete next[item.id]; return next }) }} />
        <span className="break-all font-mono text-xs text-[var(--muted)]">{item.gateway_mac}</span>
        <Checkbox checked={item.disable_proxy} onChange={(event) => replaceNetwork(item.id, { disable_proxy: event.target.checked })}>{zh ? '公网直连' : 'Direct public traffic'}</Checkbox>
        <Button shape="circle" variant="text" title={zh ? '删除网络' : 'Remove network'} aria-label={zh ? '删除网络' : 'Remove network'} icon={<Trash2 size={15} />} onClick={() => save({ known_networks: settings!.known_networks.filter((network) => network.id !== item.id) })} />
      </div>)}
      {settings && (settings.known_networks?.length ?? 0) === 0 ? <p className="rounded-lg border border-dashed border-[var(--border)] px-3 py-5 text-center text-sm text-[var(--muted)]">{zh ? '尚未添加网络。请在目标网络中打开本页并点击“将当前网络加入”。' : 'No networks yet. Open this page on the target network and add the current network.'}</p> : null}
    </div>
    {notice ? <p className="mt-3 text-xs text-[var(--muted)]">{notice}</p> : null}
  </Card>
}

function pathLabel(path: string | undefined, zh: boolean) {
  if (path === 'direct') return zh ? '公网直连' : 'Public direct'
  if (path === 'proxy') return zh ? '公网代理' : 'Public proxy'
  if (path === 'inactive') return zh ? '核心未运行' : 'Core stopped'
  return zh ? '检测中' : 'Detecting'
}
