import type { DnsConfigEditorProps } from '@acme/subscription-editor'
import { useCallback } from 'react'
import { api } from '@/lib/api'
import { useOptionalSession } from '@/lib/session'

type RouteCheck = Awaited<ReturnType<NonNullable<DnsConfigEditorProps['checkFakeIpRange']>>>

export function useFakeIpRouteCheck() {
  const session = useOptionalSession()?.session
  const check = useCallback(async (range: string, signal: AbortSignal) => {
    if (!session) throw new Error('No authenticated session')
    return api<RouteCheck>(session, '/network/fake-ip/check', {
      method: 'POST', body: JSON.stringify({ range }), signal,
    })
  }, [session])
  return session ? check : undefined
}
