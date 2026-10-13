import { Button, Input, Select, Spin, Table } from '@acme/components'
import { useState } from 'react'
import { useI18n } from '../i18n/provider'
import type { MessageKey } from '../i18n/zh-CN'
import { relayEgressApi, type RelayEgressTest, type RelayOutbound } from './relay-egress-api'
import { useRelayAutosave, type RelayDraft } from './use-relay-autosave'

const outboundValue = (outbound: RelayOutbound) => outbound.type === 'proxy' ? `proxy:${outbound.proxyId}` : outbound.type
const parseOutbound = (value: string): RelayOutbound => value.startsWith('proxy:') ? { type: 'proxy', proxyId: value.slice(6) } : { type: value as 'system' | 'direct' }

export function RelayEgressPanel() {
  const { t, number } = useI18n()
  const { form, loading, saving, error, stale, load, retry, change, dirty } = useRelayAutosave()
  const [hostDrafts, setHostDrafts] = useState<Record<string, string>>({})
  const [hostError, setHostError] = useState('')
  const [testing, setTesting] = useState(false)
  const [testError, setTestError] = useState('')
  const [url, setUrl] = useState('https://raw.githubusercontent.com/dler-io/Rules/refs/heads/main/Clash/Provider/AdBlock.yaml')
  const [result, setResult] = useState<RelayEgressTest | null>(null)
  const hasHostDraft = form?.hostRules.some(rule => (hostDrafts[rule.id] ?? rule.host).trim().toLowerCase() !== rule.host) ?? false
  const test = async () => {
    setTesting(true); setTestError(''); setResult(null)
    try { setResult(await relayEgressApi.test(url)) }
    catch (reason) { setTestError(reason instanceof Error ? reason.message : String(reason)) }
    finally { setTesting(false) }
  }
  const commitHost = (rule: RelayDraft['hostRules'][number], raw: string) => {
    const host = raw.trim().toLowerCase()
    setHostDrafts(current => ({ ...current, [rule.id]: host }))
    if (!host && rule.host) { setHostError(t('relay.hostRequired')); return }
    setHostError('')
    change(current => ({ ...current, hostRules: current.hostRules.map(item => item.id === rule.id ? { ...item, host } : item) }))
  }
  const select = (outbound: RelayOutbound, onChange: (outbound: RelayOutbound) => void, label: string) => {
    const proxies = form?.availableProxies ?? []
    const options = [
      { value: 'system', label: t('proxies.system') },
      { value: 'direct', label: t('relay.direct') },
      ...proxies.map((proxy) => ({ value: `proxy:${proxy.id}`, label: `${proxy.name} (${t(`proxies.status.${proxy.status}` as MessageKey)})`, disabled: !proxy.enabled })),
    ]
    if (outbound.type === 'proxy' && !proxies.some((proxy) => proxy.id === outbound.proxyId)) options.push({ value: outboundValue(outbound), label: t('proxies.unavailable'), disabled: true })
    return <Select aria-label={label} className="w-full min-w-0" value={outboundValue(outbound)} options={options} disabled={testing || loading || stale} onChange={(value) => onChange(parseOutbound(value))} />
  }
  return <div className="min-w-0 space-y-6 pt-4">
    <p className="text-sm text-[var(--muted)]">{t('relay.description')}</p>
    {loading ? <Spin /> : null}
    {error || hostError || testError ? <p role="alert" className="break-words text-sm text-red-600">{error || hostError || testError}</p> : null}
    {stale ? <p role="alert" className="text-sm">{t('relay.stale')}</p> : null}
    {stale || (!form && !loading) ? <Button disabled={saving || loading} onClick={() => { setHostDrafts({}); setHostError(''); void load() }}>{t('relay.reload')}</Button> : error && dirty ? <Button disabled={saving} onClick={() => void retry()}>{t('common.retry')}</Button> : null}
    {form ? <>
      <div className="grid items-start gap-3 sm:grid-cols-[minmax(0,1fr)_minmax(12rem,20rem)]">
        <div><h2 className="text-sm font-semibold">{t('relay.default')}</h2><p className="mt-1 text-xs text-[var(--muted)]">{t('relay.fallbackHint')}</p></div>
        {select(form.defaultOutbound, defaultOutbound => change(current => ({ ...current, defaultOutbound })), t('relay.default'))}
      </div>
      <section className="min-w-0 space-y-3">
        <div className="flex flex-wrap items-center justify-between gap-2"><div><h2 className="text-sm font-semibold">{t('relay.hostRules')}</h2><p className="mt-1 text-xs text-[var(--muted)]">{t('relay.hostHint')} {t('proxies.autoSaveHint')}</p></div><Button size="small" disabled={testing || loading || stale} onClick={() => change(current => ({ ...current, hostRules: [...current.hostRules, { id: crypto.randomUUID(), host: '', outbound: current.defaultOutbound }] }))}>{t('relay.addHost')}</Button></div>
        <Table<RelayDraft['hostRules'][number]> size="small" rowKey="id" dataSource={form.hostRules} pagination={false} scroll={{ x: 560 }} locale={{ emptyText: t('relay.empty') }} columns={[
          { title: t('relay.host'), minWidth: 220, render: (_, rule) => <Input aria-label={`${t('relay.host')} ${rule.host || t('relay.hostPlaceholder')}`} placeholder={t('relay.hostPlaceholder')} value={hostDrafts[rule.id] ?? rule.host} disabled={testing || loading || stale} onChange={event => setHostDrafts(current => ({ ...current, [rule.id]: event.target.value }))} onBlur={event => commitHost(rule, event.target.value)} /> },
          { title: t('relay.outbound'), minWidth: 210, render: (_, rule) => select(rule.outbound, outbound => change(current => ({ ...current, hostRules: current.hostRules.map(item => item.id === rule.id ? { ...item, outbound } : item) })), `${t('relay.outbound')} ${rule.host || t('relay.hostPlaceholder')}`) },
          { title: t('common.actions'), width: 80, render: (_, rule) => <Button size="small" variant="text" danger disabled={testing || loading || stale} onClick={() => { setHostError(''); change(current => ({ ...current, hostRules: current.hostRules.filter(item => item.id !== rule.id) })) }}>{t('common.delete')}</Button> },
        ]} />
      </section>
      <section className="space-y-3 border-t border-[var(--border)] pt-4">
        <div><h2 className="text-sm font-semibold">{t('relay.test')}</h2><p className="mt-1 text-xs text-[var(--muted)]">{t('relay.testHint')}</p></div>
        <div className="flex flex-col gap-2 sm:flex-row sm:items-end">
          <label className="flex min-w-0 flex-1 flex-col gap-1 text-sm"><span>{t('relay.url')}</span><Input value={url} placeholder={t('relay.urlPlaceholder')} disabled={testing || loading} onChange={event => { setUrl(event.target.value); setResult(null) }} /></label>
          <Button className="shrink-0 self-end" disabled={dirty || hasHostDraft || !!hostError || stale || saving || testing || loading || !url.trim()} loading={testing} onClick={() => void test()}>{t('relay.testAction')}</Button>
        </div>
        {result ? <dl role="status" className="grid grid-cols-[6rem_minmax(0,1fr)] gap-x-4 gap-y-2 text-sm">
          <dt className="text-[var(--muted)]">{t('relay.host')}</dt><dd className="min-w-0 break-all">{result.host}</dd>
          <dt className="text-[var(--muted)]">{t('relay.matched')}</dt><dd className="min-w-0 break-all">{result.matchedHost ?? t('relay.default')}</dd>
          <dt className="text-[var(--muted)]">{t('relay.outbound')}</dt><dd className="min-w-0 break-all">{result.outbound.type === 'system' ? t('proxies.system') : result.outbound.type === 'direct' ? t('relay.direct') : result.outboundName}</dd>
          <dt className="text-[var(--muted)]">HTTP</dt><dd>{result.httpStatus ?? '—'}</dd>
          <dt className="text-[var(--muted)]">{t('relay.elapsed')}</dt><dd>{number(result.elapsedMs)} ms</dd>
          <dt className="text-[var(--muted)]">{t('relay.bytes')}</dt><dd>{number(result.bodyBytes)} B</dd>
          <dt className="text-[var(--muted)]">{t('common.status')}</dt><dd>{result.ok ? t('relay.success') : t('relay.failure')}</dd>
          {result.error ? <><dt className="text-[var(--muted)]">{t('relay.error')}</dt><dd className="min-w-0 break-all text-red-600">{result.error}</dd></> : null}
        </dl> : null}
      </section>
    </> : null}
  </div>
}
