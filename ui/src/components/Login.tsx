import { useState, type FormEvent } from 'react'
import { ArrowRight, Server } from 'lucide-react'
import { useToast } from '@acme/components'
import { login } from '../lib/api'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import { Button, Field, Input, Spinner } from './ui'

export function Login() {
  const { t } = useI18n()
  const { setSession } = useSession()
  const toast = useToast()
  const [address, setAddress] = useState(() => window.location.origin)
  const [password, setPassword] = useState('')
  const [busy, setBusy] = useState(false)

  async function submit(event: FormEvent) {
    event.preventDefault()
    setBusy(true)
    try {
      setSession(await login(address, password))
    } catch (reason) {
      toast.error(reason instanceof Error ? reason.message : String(reason))
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
          <Button className="w-full" variant="primary" disabled={busy}>
            {busy ? <Spinner /> : <ArrowRight size={16} />}{busy ? t('connecting') : t('connect')}
          </Button>
        </form>
      </div>
    </main>
  )
}
