export type ThemePreference = 'system' | 'light' | 'dark'
export type ResolvedTheme = 'light' | 'dark'

const storageKey = 'sempre.site.theme'

export function normalizeTheme(value: string | null): ThemePreference {
  return value === 'light' || value === 'dark' ? value : 'system'
}

export function resolveTheme(preference: ThemePreference, systemDark: boolean): ResolvedTheme {
  if (preference === 'system') return systemDark ? 'dark' : 'light'
  return preference
}

export function initTheme() {
  const root = document.documentElement
  const media = window.matchMedia('(prefers-color-scheme: dark)')
  let preference = normalizeTheme(localStorage.getItem(storageKey))

  function apply(next: ThemePreference, persist: boolean) {
    preference = next
    const resolved = resolveTheme(preference, media.matches)
    root.dataset.themePreference = preference
    root.dataset.theme = resolved
    root.style.colorScheme = resolved
    if (persist) localStorage.setItem(storageKey, preference)

    const themeColor = document.querySelector<HTMLMetaElement>('meta[name="theme-color"]')
    const nextColor = resolved === 'dark' ? themeColor?.dataset.dark : themeColor?.dataset.light
    if (themeColor && nextColor) themeColor.content = nextColor

    document.querySelectorAll<HTMLImageElement>('[data-theme-image]').forEach((image) => {
      const source = resolved === 'dark' ? image.dataset.darkSrc : image.dataset.lightSrc
      if (source && image.getAttribute('src') !== source) image.src = source
    })
    document.querySelectorAll<HTMLElement>('[data-theme-option]').forEach((option) => {
      const selected = option.dataset.themeOption === preference
      option.setAttribute('aria-checked', String(selected))
    })
  }

  const trigger = document.querySelector<HTMLElement>('[data-theme-trigger]')
  const menu = document.querySelector<HTMLElement>('[data-theme-menu]')

  function closeMenu() {
    if (!menu || !trigger) return
    menu.hidden = true
    trigger.setAttribute('aria-expanded', 'false')
  }

  trigger?.addEventListener('click', () => {
    if (!menu) return
    menu.hidden = !menu.hidden
    trigger.setAttribute('aria-expanded', String(!menu.hidden))
  })
  document.querySelectorAll<HTMLElement>('[data-theme-option]').forEach((option) => {
    option.addEventListener('click', () => {
      apply(normalizeTheme(option.dataset.themeOption ?? null), true)
      closeMenu()
      trigger?.focus()
    })
  })
  document.addEventListener('pointerdown', (event) => {
    if (!menu || menu.hidden || !(event.target instanceof Node)) return
    if (!menu.contains(event.target) && !trigger?.contains(event.target)) closeMenu()
  })
  document.addEventListener('keydown', (event) => {
    if (event.key !== 'Escape' || menu?.hidden) return
    closeMenu()
    trigger?.focus()
  })
  media.addEventListener('change', () => {
    if (preference === 'system') apply(preference, false)
  })

  apply(preference, false)
}
