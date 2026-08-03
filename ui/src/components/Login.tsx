import { useState, type FormEvent } from 'react'
import { AlertTriangle, ArrowRight, Server } from 'lucide-react'
import { login } from '../lib/api'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import { Button, Field, Input, Spinner } from './ui'

export function Login() {
  const { t } = useI18n()
  const { setSession } = useSession()
  const [address, setAddress] = useState(() => window.location.origin)
  const [password, setPassword] = useState('')
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)

  async function submit(event: FormEvent) {
    event.preventDefault()
    setBusy(true)
    setError('')
    try {
      setSession(await login(address, password))
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setBusy(false)
    }
  }

  return (
    <main className="grid min-h-screen place-items-center bg-[var(--background)] px-5 py-10 text-[var(--text)]">
      <div className="w-full max-w-md">
        <div className="mb-10 flex items-center gap-3">
          <span className="grid size-11 place-items-center rounded-lg bg-emerald-600 text-white"><Server size={22} /></span>
          <div><h1 className="text-2xl font-semibold">Sempre</h1><p className="text-sm text-[var(--muted)]">Control plane</p></div>
        </div>
        <h2 className="text-xl font-semibold">{t('loginLead')}</h2>
        <p className="mt-2 text-sm leading-6 text-[var(--muted)]">{t('addressHint')}</p>
        <form className="mt-7 grid gap-5" onSubmit={submit}>
          <Field label={t('address')}>
            <Input value={address} onChange={(event) => setAddress(event.target.value)} inputMode="url" autoCapitalize="none" required />
          </Field>
          <Field label={t('password')}>
            <Input value={password} onChange={(event) => setPassword(event.target.value)} type="password" autoComplete="current-password" />
          </Field>
          {error ? <div role="alert" className="flex gap-2 border-l-2 border-red-500 bg-red-500/8 px-3 py-2 text-sm text-red-600"><AlertTriangle className="mt-0.5 shrink-0" size={16} />{error}</div> : null}
          <Button className="w-full" variant="primary" disabled={busy}>
            {busy ? <Spinner /> : <ArrowRight size={16} />}{busy ? t('connecting') : t('connect')}
          </Button>
        </form>
      </div>
    </main>
  )
}
