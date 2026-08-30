import { useEffect, useState } from 'react'
import { Card, PageTitle, Spinner } from '../../components/ui'
import { ServerCustomNodes } from './ServerCustomNodes'
import { serverAPI, type ServerCustomNode, type ServerMember, type ServerProfile, type ServerSession } from './server-api'
import { useServerLocaleText } from './server-i18n'

export function ServerCustomNodesPage({ session }: { session: ServerSession }) {
  const t = useServerLocaleText({ title: '自定义节点', detail: '维护多个订阅配置可以复用的节点。' }, { title: 'Custom nodes', detail: 'Maintain nodes that can be reused across subscription profiles.' })
  const [nodes, setNodes] = useState<ServerCustomNode[]>([])
  const [members, setMembers] = useState<ServerMember[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  useEffect(() => {
    let cancelled = false
    void Promise.all([
      serverAPI<ServerCustomNode[]>(session, '/custom-nodes'),
      serverAPI<ServerProfile[]>(session, '/profiles'),
    ]).then(async ([nextNodes, profiles]) => {
      const memberLists = await Promise.all(profiles.filter((profile) => profile.role === 'owner').map((profile) => serverAPI<ServerMember[]>(session, `/profiles/${profile.id}/members`)))
      if (cancelled) return
      setNodes(nextNodes)
      setMembers(uniqueMembers(memberLists.flat()))
    }).catch((reason: Error) => setError(reason.message)).finally(() => setLoading(false))
    return () => { cancelled = true }
  }, [session])
  return <div className="space-y-5">
    <PageTitle title={t.title} detail={t.detail} />
    {error ? <p role="alert" className="border-l-2 border-red-500 px-3 py-2 text-sm text-red-700 dark:text-red-300">{error}</p> : null}
    {loading ? <Card className="grid min-h-52 place-items-center"><Spinner /></Card> : <ServerCustomNodes session={session} nodes={nodes} members={members} onChange={setNodes} />}
  </div>
}

function uniqueMembers(members: ServerMember[]) {
  return [...new Map(members.map((member) => [member.user_id, member])).values()].sort((left, right) => left.email.localeCompare(right.email))
}
