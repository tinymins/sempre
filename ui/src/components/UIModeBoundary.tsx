import type { ReactNode } from 'react'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import { useUIMode } from '../lib/uiMode'
import { Button, Spinner } from './ui'

export function UIModeBoundary({ children }: { children: ReactNode }) {
  const { session } = useSession()
  const { locale } = useI18n()
  const settings = useUIMode()
  if (!session) return children
  if (settings.isPending) return <div className="grid min-h-screen place-items-center"><Spinner /></div>
  if (settings.isError) return <div role="alert" className="grid min-h-screen place-content-center justify-items-center gap-3 p-6">
    <p>{locale === 'zh-CN' ? '读取服务端界面设置失败' : 'Failed to load server UI settings'}</p>
    <p className="text-sm text-[var(--muted)]">{settings.error.message}</p>
    <Button onClick={() => void settings.refetch()}>{locale === 'zh-CN' ? '重试' : 'Retry'}</Button>
  </div>
  return children
}
