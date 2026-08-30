import { useState, type FormEvent } from 'react'
import { Button, Card, Field, Input, Spinner } from '../../components/ui'
import { serverAuthenticate, saveServerSession, type ServerSession } from './server-api'
import { useServerT } from './server-i18n'

export function ServerAuth({ onAuthenticated }: { onAuthenticated: (session: ServerSession) => void }) {
  const t = useServerT()
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
        <p className="mt-1 text-sm text-[var(--muted)]">{t('serverSubtitle')}</p>
        <form className="mt-6 space-y-4" onSubmit={submit}>
          <Field label={t('email')}><Input type="email" autoComplete="email" value={email} onChange={(event) => setEmail(event.target.value)} /></Field>
          <Field label={t('password')} hint={mode === 'register' ? t('passwordHint') : undefined}><Input type="password" autoComplete={mode === 'register' ? 'new-password' : 'current-password'} value={password} onChange={(event) => setPassword(event.target.value)} /></Field>
          {error ? <p role="alert" className="text-sm text-red-600 dark:text-red-400">{error}</p> : null}
          <Button className="w-full" variant="primary" disabled={pending || !email.trim() || password.length < (mode === 'register' ? 12 : 1)}>{pending ? <Spinner /> : null}{t(mode === 'login' ? 'signIn' : 'createAccount')}</Button>
        </form>
        <Button className="mt-3 w-full" variant="ghost" disabled={pending} onClick={() => { setMode(mode === 'login' ? 'register' : 'login'); setError('') }}>
          {t(mode === 'login' ? 'createAccountLink' : 'alreadyAccount')}
        </Button>
      </Card>
    </main>
  )
}
