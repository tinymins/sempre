import { Button, Card, Input, Password, useToast } from '@acme/components'
import { KeyRound } from 'lucide-react'
import { useEffect, useState, type FormEvent } from 'react'
import { authConfig, login, register, type ServerUser } from './server-api'
import { useI18n } from './i18n/provider'

export function ServerAuth({ onAuthenticated }: { onAuthenticated: (user: ServerUser) => void }) {
  const { t } = useI18n()
  const toast = useToast()
  const [email, setEmail] = useState('')
  const [name, setName] = useState('')
  const [password, setPassword] = useState('')
  const [invitationCode, setInvitationCode] = useState(new URLSearchParams(window.location.search).get('invite') ?? '')
  const [mode, setMode] = useState<'login' | 'register'>(invitationCode ? 'register' : 'login')
  const [registration, setRegistration] = useState<{ allowRegistration: boolean; firstUser: boolean } | null>(null)
  const [pending, setPending] = useState(false)
  const [error, setError] = useState('')

  useEffect(() => {
    let active = true
    void authConfig().then((config) => {
      if (!active) return
      setRegistration(config)
      if (config.firstUser) setMode('register')
    }).catch((reason) => { if (active) setError(reason instanceof Error ? reason.message : String(reason)) })
    return () => { active = false }
  }, [])

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    if (mode === 'register' && password.length < 12) { setError(t('auth.passwordMin')); return }
    setPending(true)
    setError('')
    try {
      onAuthenticated(mode === 'register' ? await register(name.trim(), email.trim(), password, invitationCode.trim() || undefined) : await login(email.trim(), password))
    } catch (reason) {
      toast.error(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setPending(false)
    }
  }

  return (
    <main className="grid min-h-screen place-items-center bg-[var(--background)] px-4 py-10 text-[var(--text)]">
      <div className="w-full max-w-sm">
        <div className="mb-6 flex flex-col items-center text-center">
          <div className="mb-3 grid size-12 place-items-center rounded-2xl bg-emerald-500/12 text-emerald-600 dark:text-emerald-400">
            <KeyRound size={23} />
          </div>
          <h1 className="text-2xl font-semibold tracking-tight">Sempre Server</h1>
          <p className="mt-1 text-sm text-[var(--muted)]">{registration?.firstUser ? t('auth.firstAdmin') : t('auth.serverSubtitle')}</p>
        </div>
        <Card>
          <form className="space-y-4" onSubmit={submit}>
            {mode === 'register' ? <label className="block space-y-1.5" htmlFor="server-name">
              <span className="text-sm font-medium">{t('account.name')}</span>
              <Input id="server-name" size="large" autoComplete="name" value={name} onChange={(event) => setName(event.target.value)} />
            </label> : null}
            <label className="block space-y-1.5" htmlFor="server-email">
              <span className="text-sm font-medium">{t('common.email')}</span>
              <Input
                id="server-email"
                type="email"
                size="large"
                autoComplete="email"
                autoFocus
                value={email}
                onChange={(event) => setEmail(event.target.value)}
              />
            </label>
            <label className="block space-y-1.5" htmlFor="server-password">
              <span className="text-sm font-medium">{t('common.password')}</span>
              <Password
                id="server-password"
                size="large"
                autoComplete={mode === 'register' ? 'new-password' : 'current-password'}
                value={password}
                onChange={(event) => setPassword(event.target.value)}
              />
            </label>
            {mode === 'register' && !registration?.firstUser ? <label className="block space-y-1.5" htmlFor="server-invite"><span className="text-sm font-medium">{t('auth.inviteCode')}{registration?.allowRegistration ? t('auth.optional') : ''}</span><Input id="server-invite" value={invitationCode} onChange={(event) => setInvitationCode(event.target.value)} /></label> : null}
            {error ? <p role="alert" className="text-sm text-red-600 dark:text-red-400">{error}</p> : null}
            <Button
              block
              htmlType="submit"
              loading={pending}
              size="large"
              variant="primary"
              disabled={!email.trim() || !password || (mode === 'register' && (!name.trim() || password.length < 12 || (!registration?.firstUser && !registration?.allowRegistration && !invitationCode.trim())))}
            >
              {mode === 'register' ? t('auth.createAndLogin') : t('auth.signIn')}
            </Button>
          </form>
          {registration ? <Button block className="mt-3" variant="text" onClick={() => { setMode(mode === 'login' ? 'register' : 'login'); setError('') }}>
            {mode === 'login' ? registration.allowRegistration || registration.firstUser ? t('auth.register') : t('auth.invitedRegister') : t('auth.backToLogin')}
          </Button> : null}
        </Card>
      </div>
    </main>
  )
}
