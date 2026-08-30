import { useEffect, useState } from 'react'
import { Select } from '@acme/components'
import { Card, EmptyState, Field, PageTitle, Spinner } from '../../components/ui'
import type { SubscriptionTarget } from '../../lib/types'
import { ServerDiagnostics } from './ServerDiagnostics'
import { serverAPI, serverTargets, type ServerProfile, type ServerSession } from './server-api'
import { useServerLocaleText } from './server-i18n'

export function ServerNetworkTools({ session }: { session: ServerSession }) {
  const t = useServerLocaleText({ title: '网络测试', detail: '测试订阅源抓取、境内代理路径和节点转换过程。', profile: '订阅配置', target: '输出目标', empty: '没有可测试的订阅配置', emptyDetail: '请先在订阅配置页面创建配置并添加订阅源。' }, { title: 'Network test', detail: 'Test source fetching, the configured mainland proxy path, and node conversion.', profile: 'Subscription profile', target: 'Output target', empty: 'No profile available to test', emptyDetail: 'Create a subscription profile and add a source first.' })
  const [profiles, setProfiles] = useState<ServerProfile[]>([])
  const [profileId, setProfileId] = useState('')
  const [targets, setTargets] = useState<SubscriptionTarget[]>([])
  const [target, setTarget] = useState('sing-box-v13')
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  useEffect(() => {
    let cancelled = false
    void Promise.all([serverAPI<ServerProfile[]>(session, '/profiles'), serverTargets()]).then(([nextProfiles, nextTargets]) => {
      if (cancelled) return
      setProfiles(nextProfiles)
      setProfileId(nextProfiles[0]?.id ?? '')
      setTargets(nextTargets)
      setTarget(nextTargets[0]?.format ?? 'sing-box-v13')
    }).catch((reason: Error) => setError(reason.message)).finally(() => setLoading(false))
    return () => { cancelled = true }
  }, [session])
  const profile = profiles.find((item) => item.id === profileId)
  return <div className="space-y-5">
    <PageTitle title={t.title} detail={t.detail} />
    {error ? <p role="alert" className="border-l-2 border-red-500 px-3 py-2 text-sm text-red-700 dark:text-red-300">{error}</p> : null}
    {loading ? <Card className="grid min-h-52 place-items-center"><Spinner /></Card> : profiles.length ? <>
      <Card className="flex flex-wrap items-end gap-3 p-4"><Field label={t.profile}><Select className="min-w-64" value={profileId} options={profiles.map((item) => ({ value: item.id, label: item.name }))} onChange={(value) => setProfileId(String(value))} /></Field><Field label={t.target}><Select className="min-w-56" value={target} options={targets.map((item) => ({ value: item.format, label: item.format }))} onChange={(value) => setTarget(String(value))} /></Field></Card>
      {profile ? <ServerDiagnostics session={session} profileId={profile.id} sources={profile.document.sources} target={target} targets={targets} /> : null}
    </> : <EmptyState title={t.empty} detail={t.emptyDetail} />}
  </div>
}
