import { useEffect, useState } from 'react'
import { Clock3, Rss, Server as ServerIcon, Share2 } from 'lucide-react'
import { useNavigate } from 'react-router-dom'
import { Empty } from '@acme/components'
import { Badge, Button, Card, PageTitle, Spinner } from '../../components/ui'
import { serverAPI, type ServerProfile, type ServerSession, type ServerUserStats } from './server-api'
import { useServerLocaleText } from './server-i18n'

export function ServerOverview({ session }: { session: ServerSession }) {
  const navigate = useNavigate()
  const t = useServerLocaleText({ title: '概览', welcome: '欢迎回来，', online: '服务在线', profiles: '订阅配置', nodes: '已发布节点', requests: '今日订阅请求', recent: '最近配置', recentDetail: '继续编辑或发布已有订阅配置。', viewAll: '查看全部', revision: '版本', empty: '还没有订阅配置。', quick: '快捷操作', manageProfiles: '管理订阅配置', manageProfilesDetail: '编辑订阅源、规则、分组与输出目标', manageNodes: '管理自定义节点', manageNodesDetail: '维护多个配置可复用的节点', access: '成员与分享', accessDetail: '配置协作者权限和公开订阅链接' }, { title: 'Overview', welcome: 'Welcome back, ', online: 'Server online', profiles: 'Subscription profiles', nodes: 'Published nodes', requests: 'Requests today', recent: 'Recent profiles', recentDetail: 'Continue editing or publishing an existing profile.', viewAll: 'View all', revision: 'Revision', empty: 'No subscription profiles yet.', quick: 'Quick actions', manageProfiles: 'Manage subscriptions', manageProfilesDetail: 'Edit sources, rules, groups, and output targets', manageNodes: 'Manage custom nodes', manageNodesDetail: 'Maintain reusable nodes across profiles', access: 'Members and shares', accessDetail: 'Configure collaborators and public subscription links' })
  const [profiles, setProfiles] = useState<ServerProfile[]>([])
  const [stats, setStats] = useState<ServerUserStats | null>(null)
  const [error, setError] = useState('')
  useEffect(() => {
    let cancelled = false
    void Promise.all([
      serverAPI<ServerProfile[]>(session, '/profiles'),
      serverAPI<ServerUserStats>(session, '/stats'),
    ]).then(([nextProfiles, nextStats]) => {
      if (cancelled) return
      setProfiles(nextProfiles)
      setStats(nextStats)
    }).catch((reason: Error) => setError(reason.message))
    return () => { cancelled = true }
  }, [session])

  return <div className="space-y-6">
    <PageTitle title={t.title} detail={`${t.welcome}${session.user.email}`}><Badge tone="success">{t.online}</Badge></PageTitle>
    {error ? <p role="alert" className="border-l-2 border-red-500 px-3 py-2 text-sm text-red-700 dark:text-red-300">{error}</p> : null}
    {!stats ? <Card className="grid min-h-40 place-items-center"><Spinner /></Card> : <>
      <div className="grid gap-3 sm:grid-cols-3">
        <Metric icon={Rss} label={t.profiles} value={stats.total_profiles} />
        <Metric icon={ServerIcon} label={t.nodes} value={stats.total_nodes} />
        <Metric icon={Clock3} label={t.requests} value={stats.today_requests} />
      </div>
      <div className="grid gap-4 lg:grid-cols-[minmax(0,3fr)_minmax(18rem,2fr)]">
        <Card className="p-5">
          <div className="flex items-start justify-between gap-4"><div><h2 className="font-semibold">{t.recent}</h2><p className="mt-1 text-xs text-[var(--muted)]">{t.recentDetail}</p></div><Button onClick={() => navigate('/subscriptions')}>{t.viewAll}</Button></div>
          <div className="mt-4 divide-y divide-[var(--border)]">
            {profiles.slice(0, 5).map((profile) => <button key={profile.id} type="button" className="flex w-full items-center gap-3 py-3 text-left hover:text-emerald-600" onClick={() => navigate(`/subscriptions/${profile.id}`)}><span className="grid size-9 place-items-center rounded-md bg-emerald-500/10 text-emerald-600"><Rss size={17} /></span><span className="min-w-0 flex-1"><span className="block truncate text-sm font-medium">{profile.name}</span><span className="block text-xs text-[var(--muted)]">{t.revision} {profile.revision} · {new Date(profile.updated_at).toLocaleString()}</span></span><Badge tone={profile.role === 'viewer' ? 'neutral' : 'info'}>{profile.role}</Badge></button>)}
            {!profiles.length ? <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t.empty} /> : null}
          </div>
        </Card>
        <Card className="p-5">
          <h2 className="font-semibold">{t.quick}</h2>
          <div className="mt-3 space-y-2">
            <QuickAction icon={Rss} title={t.manageProfiles} detail={t.manageProfilesDetail} onClick={() => navigate('/subscriptions')} />
            <QuickAction icon={ServerIcon} title={t.manageNodes} detail={t.manageNodesDetail} onClick={() => navigate('/custom-nodes')} />
            <QuickAction icon={Share2} title={t.access} detail={t.accessDetail} onClick={() => navigate('/management')} />
          </div>
        </Card>
      </div>
    </>}
  </div>
}

function Metric({ icon: Icon, label, value }: { icon: typeof Rss; label: string; value: number }) {
  return <Card className="p-4"><span className="grid size-8 place-items-center rounded-md bg-emerald-500/10 text-emerald-600"><Icon size={17} /></span><p className="mt-5 text-xs text-[var(--muted)]">{label}</p><p className="mt-1 text-lg font-semibold tabular-nums">{value}</p></Card>
}

function QuickAction({ icon: Icon, title, detail, onClick }: { icon: typeof Rss; title: string; detail: string; onClick: () => void }) {
  return <button type="button" className="flex w-full items-center gap-3 rounded-md p-3 text-left hover:bg-[var(--surface-hover)]" onClick={onClick}><span className="grid size-9 shrink-0 place-items-center rounded-md bg-emerald-500/10 text-emerald-600"><Icon size={17} /></span><span><span className="block text-sm font-medium">{title}</span><span className="block text-xs text-[var(--muted)]">{detail}</span></span></button>
}
