import { useEffect, useRef } from 'react'

const POLL_INTERVAL_MS = 5000

export function ServiceVersionReload() {
  useServiceVersionChange(() => window.location.reload())
  return null
}

export function useServiceVersionChange(onChange: () => void) {
  const onChangeRef = useRef(onChange)
  useEffect(() => {
    onChangeRef.current = onChange
  }, [onChange])

  useEffect(() => {
    const controller = new AbortController()
    let baseline = ''
    let timer: ReturnType<typeof setTimeout> | undefined

    const poll = async () => {
      try {
        const response = await fetch(`${window.location.origin}/api/v1/health`, {
          cache: 'no-store',
          signal: controller.signal,
        })
        if (response.ok) {
          const health = await response.json() as { version?: string }
          const version = health.version?.trim() || ''
          if (version && baseline && version !== baseline) {
            onChangeRef.current()
            return
          }
          if (version) baseline = version
        }
      } catch {
        // The service is expected to be briefly unavailable while it upgrades.
      }
      if (!controller.signal.aborted) timer = setTimeout(poll, POLL_INTERVAL_MS)
    }

    void poll()
    return () => {
      controller.abort()
      if (timer) clearTimeout(timer)
    }
  }, [])
}
