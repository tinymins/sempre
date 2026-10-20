import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react'

export function useTraceFollow(count: number, running: boolean) {
  const viewportRef = useRef<HTMLDivElement>(null)
  const lastScrollRef = useRef(0)
  const lastTouchRef = useRef<number | null>(null)
  const [following, setFollowing] = useState(true)
  const scrollLatest = useCallback(() => {
    const viewport = viewportRef.current
    if (viewport) viewport.scrollTo({ top: viewport.scrollHeight, behavior: window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth' })
  }, [])
  useLayoutEffect(() => {
    if (following && count) scrollLatest()
  }, [count, running, following, scrollLatest])

  const pause = useCallback(() => {
    setFollowing(false)
    const viewport = viewportRef.current
    if (viewport) viewport.scrollTo({ top: viewport.scrollTop, behavior: 'instant' })
  }, [])

  useEffect(() => {
    const viewport = viewportRef.current
    if (!viewport) return
    const wheel = (event: WheelEvent) => { if (event.deltaY < 0) pause() }
    const key = (event: KeyboardEvent) => { if (['ArrowUp', 'PageUp', 'Home'].includes(event.key)) pause() }
    const touchStart = (event: TouchEvent) => { lastTouchRef.current = event.touches[0]?.clientY ?? null }
    const touchMove = (event: TouchEvent) => {
      const y = event.touches[0]?.clientY ?? null
      if (y !== null && lastTouchRef.current !== null && y > lastTouchRef.current + 4) pause()
      lastTouchRef.current = y
    }
    const scroll = () => {
      if (viewport.scrollTop < lastScrollRef.current - 2) pause()
      lastScrollRef.current = viewport.scrollTop
    }
    viewport.addEventListener('wheel', wheel, { passive: true })
    viewport.addEventListener('keydown', key)
    viewport.addEventListener('touchstart', touchStart, { passive: true })
    viewport.addEventListener('touchmove', touchMove, { passive: true })
    viewport.addEventListener('scroll', scroll, { passive: true })
    return () => {
      viewport.removeEventListener('wheel', wheel)
      viewport.removeEventListener('keydown', key)
      viewport.removeEventListener('touchstart', touchStart)
      viewport.removeEventListener('touchmove', touchMove)
      viewport.removeEventListener('scroll', scroll)
    }
  }, [pause])
  return { viewportRef, following, pause, resume: () => { setFollowing(true); scrollLatest() } }
}
