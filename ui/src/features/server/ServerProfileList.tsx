import { Plus, Users } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { Badge, Button, Card, Field, Input, Spinner } from '../../components/ui'
import { newServerProfile, serverAPI, type ServerProfile, type ServerSession } from './server-api'

export function ServerProfileList({ session, onLogout }: { session: ServerSession; onLogout: () => void }) {
  const navigate = useNavigate()
  const [profiles, setProfiles] = useState<ServerProfile[]>([])
  const [name, setName] = useState('')
  const [loading, setLoading] = useState(true)
  const [pending, setPending] = useState(false)
  const [error, setError] = useState('')
  useEffect(() => {
    void serverAPI<ServerProfile[]>(session, '/profiles')
      .then(setProfiles)
      .catch((reason: Error) => setError(reason.message))
      .finally(() => setLoading(false))
  }, [session])
  const create = async () => {
    const trimmed = name.trim()
    if (!trimmed) return
    setPending(true)
    setError('')
    try {
      const profile = await serverAPI<ServerProfile>(session, '/profiles', {
        method: 'POST', body: JSON.stringify({ name: trimmed, document: newServerProfile(trimmed) }),
      })
      navigate(`/server/subscriptions/${profile.id}`)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setPending(false)
    }
  }
  return (
    <main className="mx-auto min-h-screen max-w-5xl space-y-6 p-5">
      <header className="flex flex-wrap items-start justify-between gap-3">
        <div><h1 className="text-xl font-semibold">Sempre Server</h1><p className="text-sm text-[var(--muted)]">{session.user.email}</p></div>
        <Button variant="ghost" onClick={onLogout}>Sign out</Button>
      </header>
      <Card className="p-5">
        <h2 className="font-semibold">Create profile</h2>
        <div className="mt-3 flex items-end gap-2">
          <Field label="Profile name"><Input value={name} onChange={(event) => setName(event.target.value)} /></Field>
          <Button variant="primary" disabled={pending || !name.trim()} onClick={create}>{pending ? <Spinner /> : <Plus size={16} />}Create</Button>
        </div>
        {error ? <p role="alert" className="mt-3 text-sm text-red-600 dark:text-red-400">{error}</p> : null}
      </Card>
      {loading ? <Card className="grid min-h-40 place-items-center"><Spinner /></Card> : (
        <div className="grid gap-3 md:grid-cols-2">
          {profiles.map((profile) => (
            <Card key={profile.id} className="cursor-pointer p-4 transition-colors hover:bg-[var(--surface-hover)]" role="button" tabIndex={0} onClick={() => navigate(`/server/subscriptions/${profile.id}`)} onKeyDown={(event) => { if (event.key === 'Enter') navigate(`/server/subscriptions/${profile.id}`) }}>
              <div className="flex items-start justify-between gap-2"><h2 className="font-semibold">{profile.name}</h2><Badge tone={profile.role === 'viewer' ? 'neutral' : 'info'}>{profile.role}</Badge></div>
              <p className="mt-2 text-xs text-[var(--muted)]">Revision {profile.revision} · Updated {new Date(profile.updated_at).toLocaleString()}</p>
              <p className="mt-3 flex items-center gap-1 text-sm text-[var(--muted)]"><Users size={14} />Multi-user profile</p>
            </Card>
          ))}
          {!profiles.length ? <Card className="p-8 text-center text-sm text-[var(--muted)]">No profiles yet.</Card> : null}
        </div>
      )}
    </main>
  )
}
