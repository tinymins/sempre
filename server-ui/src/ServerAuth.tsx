import { Button, Card, Input, Password } from '@acme/components'
import { KeyRound } from 'lucide-react'
import { useState, type FormEvent } from 'react'
import { login, type ServerSession } from './server-api'

export function ServerAuth({ onAuthenticated }: { onAuthenticated: (session: ServerSession) => void }) {
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [pending, setPending] = useState(false)
  const [error, setError] = useState('')

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    setPending(true)
    setError('')
    try {
      onAuthenticated(await login(email.trim(), password))
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
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
          <p className="mt-1 text-sm text-[var(--muted)]">多人订阅转换与配置管理</p>
        </div>
        <Card>
          <form className="space-y-4" onSubmit={submit}>
            <label className="block space-y-1.5" htmlFor="server-email">
              <span className="text-sm font-medium">邮箱</span>
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
              <span className="text-sm font-medium">密码</span>
              <Password
                id="server-password"
                size="large"
                autoComplete="current-password"
                value={password}
                onChange={(event) => setPassword(event.target.value)}
              />
            </label>
            {error ? <p role="alert" className="text-sm text-red-600 dark:text-red-400">{error}</p> : null}
            <Button
              block
              htmlType="submit"
              loading={pending}
              size="large"
              variant="primary"
              disabled={!email.trim() || !password}
            >
              登录
            </Button>
          </form>
        </Card>
      </div>
    </main>
  )
}
