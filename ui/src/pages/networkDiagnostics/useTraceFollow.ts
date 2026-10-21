import { useCallback, useEffect, useRef, useState } from 'react'

// The control plane scrolls the document; scrollIntoView also respects enclosing
// scroll surfaces if this panel is embedded elsewhere.
export function useTraceFollow(revision: unknown, enabled: boolean) {
  const latestRef = useRef<HTMLDivElement>(null)
  const followingRef = useRef(true)
  const [following, setFollowing] = useState(true)
  const scrollLatest = useCallback(() => {
    latestRef.current?.scrollIntoView({
      behavior: window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 'instant' : 'smooth',
      block: 'nearest',
    })
  }, [])
  const pause = useCallback(() => {
    if (!enabled) return
    followingRef.current = false
    setFollowing(false)
    // Cancel an in-flight smooth scroll before the reader inspects a node.
    window.scrollTo({ top: window.scrollY, behavior: 'instant' })
  }, [enabled])
  const resume = () => {
    followingRef.current = true
    setFollowing(true)
    scrollLatest()
  }
  useEffect(() => {
    if (!revision || !followingRef.current) return
    const frame = requestAnimationFrame(scrollLatest)
    return () => cancelAnimationFrame(frame)
  }, [revision, scrollLatest])
  useEffect(() => {
    if (!enabled) return
    let lastTouch: number | undefined
    const wheel = (event: WheelEvent) => { if (event.deltaY < 0) pause() }
    const touchStart = (event: TouchEvent) => { lastTouch = event.touches[0]?.clientY }
    const touchMove = (event: TouchEvent) => {
      const y = event.touches[0]?.clientY
      if (y !== undefined && lastTouch !== undefined && y > lastTouch + 4) pause()
      lastTouch = y
    }
    const key = (event: KeyboardEvent) => {
      if (['ArrowUp', 'PageUp', 'Home'].includes(event.key) || (event.key === ' ' && event.shiftKey)) pause()
    }
    window.addEventListener('wheel', wheel, { passive: true })
    window.addEventListener('touchstart', touchStart, { passive: true })
    window.addEventListener('touchmove', touchMove, { passive: true })
    window.addEventListener('keydown', key)
    return () => {
      window.removeEventListener('wheel', wheel)
      window.removeEventListener('touchstart', touchStart)
      window.removeEventListener('touchmove', touchMove)
      window.removeEventListener('keydown', key)
    }
  }, [enabled, pause])
  return { latestRef, following, pause, resume }
}
