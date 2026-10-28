import { useEffect } from 'react'
import { skipToken, useQuery } from '@tanstack/react-query'

export const serverIdentityKey = (baseURL: string) => ['server-identity', baseURL]

export function useServerTitle(baseURL: string) {
  // The existing health poll and settings writes populate this server-specific cache.
  const { data: name } = useQuery<string>({ queryKey: serverIdentityKey(baseURL), queryFn: skipToken })
  useEffect(() => {
    document.title = `Sempre - ${name?.trim() || new URL(baseURL).hostname}`
    return () => { document.title = 'Sempre' }
  }, [baseURL, name])
}
