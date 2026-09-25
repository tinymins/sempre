import { Button, Card, Input, Spin } from '@acme/components'
import { Copy } from 'lucide-react'
import { useEffect, useState } from 'react'
import { dashboardApi, type NetworkList } from './api'

function NetworkListCard({ title, load }: { title: string; load: () => Promise<NetworkList> }) {
  const [data, setData] = useState<NetworkList | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const [search, setSearch] = useState('')
  const [shown, setShown] = useState(200)
  const [copyError, setCopyError] = useState('')
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
    setCopyError('')
    try { await navigator.clipboard.writeText(matches.join('\n')) }
    catch { setCopyError('复制失败，请检查剪贴板权限。') }
  }
  return <Card className="space-y-3">
    <div className="flex items-baseline justify-between"><h2 className="font-semibold">{title}</h2><span className="text-xl font-semibold">{data?.count.toLocaleString() ?? '—'}</span></div>
    {loading ? <Spin /> : null}
    {error ? <p role="alert" className="text-sm text-red-600">{error} <Button size="small" onClick={() => void refresh()}>重试</Button></p> : null}
    {data ? <>
      <div className="flex gap-2"><Input value={search} onChange={(event) => { setSearch(event.target.value); setShown(200) }} placeholder="搜索列表" className="min-w-0 flex-1" /><Button icon={<Copy size={14} />} disabled={matches.length === 0} onClick={() => void copy()}>复制筛选结果</Button></div>
      {copyError ? <p role="alert" className="text-xs text-red-600">{copyError}</p> : null}
      <p className="text-xs text-[var(--muted)]">显示 {Math.min(shown, matches.length)} / {matches.length} 项</p>
      {matches.length ? <pre className="max-h-80 overflow-auto rounded border border-[var(--border)] p-3 text-xs leading-5">{matches.slice(0, shown).join('\n')}</pre> : <p className="text-sm text-[var(--muted)]">没有匹配项目</p>}
      {shown < matches.length ? <Button block onClick={() => setShown((current) => current + 200)}>显示更多</Button> : null}
    </> : null}
  </Card>
}

export function NetworkToolsPage() {
  return <section className="space-y-5"><div><h1 className="text-2xl font-semibold">网络工具</h1><p className="mt-1 text-sm text-[var(--muted)]">查看和检索服务端的中国 IP 与域名列表。</p></div><div className="grid gap-4 lg:grid-cols-2"><NetworkListCard title="GeoIP CN" load={dashboardApi.geoIpCn} /><NetworkListCard title="GeoSite CN" load={dashboardApi.geoSiteCn} /></div></section>
}
