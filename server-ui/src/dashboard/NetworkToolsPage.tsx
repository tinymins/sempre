import { Button, Card, CodePanel, Input, Spin, useToast } from '@acme/components'
import { Copy } from 'lucide-react'
import { useEffect, useState } from 'react'
import { dashboardApi, type NetworkList } from './api'
import { useI18n } from '../i18n/provider'

function NetworkListCard({ title, load }: { title: string; load: () => Promise<NetworkList> }) {
  const { t, number } = useI18n()
  const toast = useToast()
  const [data, setData] = useState<NetworkList | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const [search, setSearch] = useState('')
  const [shown, setShown] = useState(200)
  const refresh = async () => {
    setLoading(true)
    setError('')
    try { setData(await load()) }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)) }
    finally { setLoading(false) }
  }
  useEffect(() => {
    let active = true
    void load().then((next) => { if (active) setData(next) })
      .catch((reason) => { if (active) setError(reason instanceof Error ? reason.message : String(reason)) })
      .finally(() => { if (active) setLoading(false) })
    return () => { active = false }
  }, [load])
  const q = search.trim().toLowerCase()
  const matches = data?.items.filter((item) => item.toLowerCase().includes(q)) ?? []
  const copy = async () => {
    try { await navigator.clipboard.writeText(matches.join('\n')); toast.success(t('network.copied', { count: number(matches.length) })) }
    catch { toast.error(t('common.copyFailed')) }
  }
  return <Card className="space-y-3">
    <div className="flex items-baseline justify-between"><h2 className="font-semibold">{title}</h2><span className="text-xl font-semibold">{data ? number(data.count) : '—'}</span></div>
    {loading ? <Spin /> : null}
    {error ? <p role="alert" className="text-sm text-red-600">{error} <Button size="small" onClick={() => void refresh()}>{t('common.retry')}</Button></p> : null}
    {data ? <>
      <div className="flex gap-2"><Input value={search} onChange={(event) => { setSearch(event.target.value); setShown(200) }} placeholder={t('network.search')} className="min-w-0 flex-1" /><Button icon={<Copy size={14} />} disabled={matches.length === 0} onClick={() => void copy()}>{t('network.copyFiltered')}</Button></div>
      <p className="text-xs text-[var(--muted)]">{t('network.shown', { shown: number(Math.min(shown, matches.length)), total: number(matches.length) })}</p>
      {matches.length ? <CodePanel language="TEXT" maxHeight={320} bodyClassName="whitespace-pre font-mono text-xs leading-5">{matches.slice(0, shown).join('\n')}</CodePanel> : <p className="text-sm text-[var(--muted)]">{t('network.noMatches')}</p>}
      {shown < matches.length ? <Button block onClick={() => setShown((current) => current + 200)}>{t('network.more')}</Button> : null}
    </> : null}
  </Card>
}

export function NetworkToolsPage() {
  const { t } = useI18n()
  return <section className="space-y-5"><div><h1 className="text-2xl font-semibold">{t('network.title')}</h1><p className="mt-1 text-sm text-[var(--muted)]">{t('network.subtitle')}</p></div><div className="grid gap-4 lg:grid-cols-2"><NetworkListCard title="GeoIP CN" load={dashboardApi.geoIpCn} /><NetworkListCard title="GeoSite CN" load={dashboardApi.geoSiteCn} /></div></section>
}
