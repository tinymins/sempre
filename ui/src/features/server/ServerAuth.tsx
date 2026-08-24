import { useState, type FormEvent } from 'react'
import { Button, Card, Field, Input, Spinner } from '../../components/ui'
import { serverAuthenticate, saveServerSession, type ServerSession } from './server-api'

export function ServerAuth({ onAuthenticated }: { onAuthenticated: (session: ServerSession) => void }) {
  const [mode, setMode] = useState<'login' | 'register'>('login')
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [pending, setPending] = useState(false)
  const [error, setError] = useState('')
  const submit = async (event: FormEvent) => {
    event.preventDefault()
    setPending(true)
    setError('')
    try {
      const session = await serverAuthenticate(mode, email, password)
      saveServerSession(session)
      onAuthenticated(session)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setPending(false)
    }
  }
  return (
    <main className="grid min-h-screen place-items-center bg-[var(--background)] p-4">
      <Card className="w-full max-w-md p-6">
        <h1 className="text-xl font-semibold">Sempre Server</h1>
        <p className="mt-1 text-sm text-[var(--muted)]">Multi-user subscription conversion and profile management</p>
        <form className="mt-6 space-y-4" onSubmit={submit}>
          <Field label="Email"><Input type="email" autoComplete="email" value={email} onChange={(event) => setEmail(event.target.value)} /></Field>
          <Field label="Password" hint={mode === 'register' ? 'Use at least 12 characters.' : undefined}><Input type="password" autoComplete={mode === 'register' ? 'new-password' : 'current-password'} value={password} onChange={(event) => setPassword(event.target.value)} /></Field>
          {error ? <p role="alert" className="text-sm text-red-600 dark:text-red-400">{error}</p> : null}
          <Button className="w-full" variant="primary" disabled={pending || !email.trim() || password.length < (mode === 'register' ? 12 : 1)}>{pending ? <Spinner /> : null}{mode === 'login' ? 'Sign in' : 'Create account'}</Button>
        </form>
        <Button className="mt-3 w-full" variant="ghost" disabled={pending} onClick={() => { setMode(mode === 'login' ? 'register' : 'login'); setError('') }}>
          {mode === 'login' ? 'Create a server account' : 'Already have an account? Sign in'}
        </Button>
      </Card>
    </main>
  )
}
