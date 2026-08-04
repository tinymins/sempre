function clamp(value: number) {
  return Math.min(1, Math.max(0, value))
}

export function initMotion() {
  const root = document.documentElement
  const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)')
  const finePointer = window.matchMedia('(hover: hover) and (pointer: fine)')

  function updateScrollState() {
    const maximum = Math.max(1, root.scrollHeight - window.innerHeight)
    root.style.setProperty('--page-progress', (window.scrollY / maximum).toFixed(4))
    document.body.classList.toggle('has-scrolled', window.scrollY > 24)
    document.querySelectorAll<HTMLElement>('[data-scene]').forEach((scene) => {
      const bounds = scene.getBoundingClientRect()
      const progress = clamp((window.innerHeight - bounds.top) / (window.innerHeight + bounds.height))
      scene.style.setProperty('--scene-progress', progress.toFixed(4))
    })
  }

  let scrollFrame = 0
  function scheduleScrollUpdate() {
    if (scrollFrame) return
    scrollFrame = window.requestAnimationFrame(() => {
      scrollFrame = 0
      updateScrollState()
    })
  }

  if (finePointer.matches && !reducedMotion.matches) {
    let pointerX = 0
    let pointerY = 0
    let pointerTargetX = 0
    let pointerTargetY = 0
    let pointerFrame = 0

    function renderPointer() {
      pointerFrame = 0
      pointerX += (pointerTargetX - pointerX) * 0.11
      pointerY += (pointerTargetY - pointerY) * 0.11
      root.style.setProperty('--pointer-x', pointerX.toFixed(4))
      root.style.setProperty('--pointer-y', pointerY.toFixed(4))
      if (Math.abs(pointerTargetX - pointerX) > 0.001 || Math.abs(pointerTargetY - pointerY) > 0.001) {
        pointerFrame = window.requestAnimationFrame(renderPointer)
      }
    }

    window.addEventListener('pointermove', (event) => {
      pointerTargetX = (event.clientX / window.innerWidth - 0.5) * 2
      pointerTargetY = (event.clientY / window.innerHeight - 0.5) * 2
      if (!pointerFrame) pointerFrame = window.requestAnimationFrame(renderPointer)
    }, { passive: true })

    document.querySelectorAll<HTMLElement>('[data-tilt]').forEach((element) => {
      element.addEventListener('pointermove', (event) => {
        const bounds = element.getBoundingClientRect()
        const localX = (event.clientX - bounds.left) / bounds.width - 0.5
        const localY = (event.clientY - bounds.top) / bounds.height - 0.5
        element.style.setProperty('--tilt-x', (localX * 2).toFixed(4))
        element.style.setProperty('--tilt-y', (localY * 2).toFixed(4))
      })
      element.addEventListener('pointerleave', () => {
        element.style.setProperty('--tilt-x', '0')
        element.style.setProperty('--tilt-y', '0')
      })
    })

    document.querySelectorAll<HTMLElement>('[data-magnetic]').forEach((element) => {
      element.addEventListener('pointermove', (event) => {
        const bounds = element.getBoundingClientRect()
        element.style.setProperty('--magnetic-x', `${(event.clientX - bounds.left - bounds.width / 2) * 0.13}px`)
        element.style.setProperty('--magnetic-y', `${(event.clientY - bounds.top - bounds.height / 2) * 0.13}px`)
      })
      element.addEventListener('pointerleave', () => {
        element.style.setProperty('--magnetic-x', '0px')
        element.style.setProperty('--magnetic-y', '0px')
      })
    })
  }

  const revealElements = document.querySelectorAll<HTMLElement>('[data-reveal]')
  if (reducedMotion.matches) {
    revealElements.forEach((element) => element.classList.add('is-visible'))
  } else {
    const observer = new IntersectionObserver((entries) => {
      entries.forEach((entry) => {
        if (!entry.isIntersecting) return
        entry.target.classList.add('is-visible')
        observer.unobserve(entry.target)
      })
    }, { threshold: 0.14 })
    revealElements.forEach((element) => observer.observe(element))
  }

  window.addEventListener('scroll', scheduleScrollUpdate, { passive: true })
  window.addEventListener('resize', scheduleScrollUpdate, { passive: true })
  updateScrollState()
  window.requestAnimationFrame(() => document.body.classList.add('is-ready'))
}
