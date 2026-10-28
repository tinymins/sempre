import { useState } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Button, Input, Select, useToast } from '@acme/components'
import { Field } from '../../components/ui'
import { api } from '../../lib/api'
import { useI18n } from '../../lib/i18n'
import { useSession } from '../../lib/session'
import { joinListen, listenNavigation, loopback, selectListenHosts, splitListen, type WebSettings } from './webListen'

export function WebListenSettings({ settings }: { settings?: WebSettings }) {
  const { locale, t } = useI18n()
  const { session } = useSession()
  const client = useQueryClient()
  const toast = useToast()
  const [draft, setDraft] = useState<{ hosts: string[]; port: string } | null>(null)
  const [accessNotice, setAccessNotice] = useState('')
  const zh = locale === 'zh-CN'
  const saved = settings?.listen_addresses.map(splitListen) ?? []
  const hosts = draft?.hosts ?? saved.map(({ host }) => host)
  const port = draft?.port ?? saved[0]?.port ?? '33211'
  const options = [
    { value: '127.0.0.1', label: zh ? '本机 — 127.0.0.1' : 'Loopback — 127.0.0.1', tagLabel: '127.0.0.1' },
    { value: '0.0.0.0', label: zh ? '所有 IPv4 地址 — 0.0.0.0' : 'All IPv4 addresses — 0.0.0.0', tagLabel: '0.0.0.0' },
    ...(settings?.interfaces ?? []).filter(({ address }) => address !== '127.0.0.1').map(({ interface: name, address }) => ({ value: address, label: `${name} — ${address}`, tagLabel: address })),
  ].filter((option, index, all) => all.findIndex((item) => item.value === option.value) === index)
  const update = useMutation({
    mutationFn: () => api<WebSettings>(session!, '/web', { method: 'PATCH', body: JSON.stringify({ listen_addresses: hosts.map((host) => joinListen(host, port)) }) }),
    onSuccess: (result) => {
      client.setQueryData(['web'], { ...settings, ...result })
      setDraft(null)
      toast.success(t('operationDone'))
      const target = listenNavigation(session!.baseURL, result.listen_addresses)
      if (!target) {
        const localOnly = result.listen_addresses.every((address) => loopback(splitListen(address).host))
        setAccessNotice(localOnly
          ? (zh ? `已保存。远程访问已关闭，请在服务器本机打开 ${result.local_url}。` : `Saved. Remote access is disabled. Open ${result.local_url} on the server itself.`)
          : (zh ? `已保存。请使用新监听范围内的服务器地址访问：${result.listen_addresses.join(', ')}。` : `Saved. Connect using a server address covered by the new listeners: ${result.listen_addresses.join(', ')}.`))
      }
      else if (target !== session!.baseURL.replace(/\/$/, '')) window.location.assign(target)
    },
    onError: (error) => toast.error(error.message),
  })
  return <div className="space-y-3">
    <div className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_7rem_auto] sm:items-end">
      <Field label={t('listenAddress')}>
        <Select mode="tags" showSearch allowClear popupMatchSelectWidth className="w-full min-w-0" aria-label={t('listenAddress')} value={hosts} options={options} disabled={!settings || update.isPending} placeholder={zh ? '选择或输入 IP，按 Enter 添加' : 'Select or enter an IP, then press Enter'} onChange={(values: string[]) => {
          try { setDraft({ hosts: selectListenHosts(hosts, values), port }); setAccessNotice('') } catch { toast.error(zh ? '请输入有效的 IPv4 或 IPv6 地址。' : 'Enter a valid IPv4 or IPv6 address.') }
        }} />
      </Field>
      <Field label={zh ? '端口' : 'Port'}><Input type="number" min={1} max={65535} value={port} disabled={!settings || update.isPending} onChange={(event) => setDraft({ hosts, port: event.target.value })} /></Field>
      <Button variant="primary" loading={update.isPending} disabled={!settings || !hosts.length || !/^\d+$/.test(port) || Number(port) < 1 || Number(port) > 65535} onClick={() => update.mutate()}>{t('apply')}</Button>
    </div>
    <p className="text-xs leading-5 text-[var(--muted)]">{zh ? '所有选中地址共用一个端口；0.0.0.0 与具体 IPv4 地址互斥。' : 'All selected addresses share one port. 0.0.0.0 cannot be combined with specific IPv4 addresses.'}</p>
    {accessNotice ? <p role="status" className="text-sm text-amber-600">{accessNotice}</p> : null}
  </div>
}
