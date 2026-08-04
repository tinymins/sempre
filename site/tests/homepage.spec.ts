import { expect, test } from '@playwright/test'

const themes = ['light', 'dark'] as const
const viewports = [
  { name: 'desktop', width: 1440, height: 900 },
  { name: 'compact-desktop', width: 1280, height: 720 },
  { name: 'mobile', width: 390, height: 844 },
  { name: 'small-mobile', width: 360, height: 800 },
]

for (const theme of themes) {
  for (const viewport of viewports) {
    test(`${theme} ${viewport.name} homepage uses the matching product interface`, async ({ page }) => {
      await page.setViewportSize(viewport)
      await page.addInitScript((selectedTheme) => localStorage.setItem('sempre.site.theme', selectedTheme), theme)
      await page.goto('/')

      await expect(page.getByRole('heading', { name: 'Sempre', level: 1 })).toBeVisible()
      await expect(page.getByText('Any core. Always current. Always running.', { exact: true }).first()).toBeVisible()
      await expect(page.locator('html')).toHaveAttribute('data-theme', theme)
      await expect(page.locator('#product')).toBeAttached()

      const dimensions = await page.evaluate((selectedTheme) => {
        const images = [...document.querySelectorAll<HTMLImageElement>('[data-theme-image]')]
        return {
          viewport: document.documentElement.clientWidth,
          document: document.documentElement.scrollWidth,
          sourcesMatch: images.every((image) => image.src.endsWith(`/assets/control-plane-${selectedTheme}.webp`)),
          widths: images.map((image) => image.naturalWidth),
        }
      }, theme)

      expect(dimensions.document).toBeLessThanOrEqual(dimensions.viewport)
      expect(dimensions.sourcesMatch).toBe(true)
      expect(dimensions.widths.length).toBe(3)
      expect(dimensions.widths.every((width) => width === 1600)).toBe(true)

      await page.evaluate(() => {
        document.body.classList.add('is-ready')
        document.querySelectorAll('[data-reveal]').forEach((element) => element.classList.add('is-visible'))
      })
      await page.waitForTimeout(1200)
      await page.screenshot({ path: `test-results/homepage/${theme}-${viewport.name}.png`, fullPage: true })
    })
  }
}

test('platform command, script link, copy, and canonical translations work together', async ({ page }) => {
  await page.goto('/')
  await page.getByRole('button', { name: 'PowerShell' }).click()
  await expect(page.locator('[data-command-output]')).toHaveText('irm https://sempre.run/install.ps1 | iex')
  await expect(page.locator('[data-script-link]')).toHaveAttribute('href', '/install.ps1')
  await page.locator('[data-copy]').click()
  await expect(page.locator('[data-copy-status]')).toHaveText('Install command copied')
  await expect(page.locator('[data-copy-status]')).toHaveClass(/is-visible/)
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('irm https://sempre.run/install.ps1 | iex')
  await page.locator('[data-language]').click()
  await expect(page.getByText('任意核心，持续更新，始终运行。', { exact: true }).first()).toBeVisible()
  await expect(page.locator('html')).toHaveAttribute('lang', 'zh-CN')
})

test('mobile Chinese content is readable without desktop center dividers', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 })
  await page.addInitScript(() => localStorage.setItem('sempre.site.locale', 'zh-CN'))
  await page.goto('/')

  await expect(page.getByRole('heading', { name: '换核心、升版本时，管理界面始终在线。' })).toBeVisible()
  await expect(page.getByRole('heading', { name: '切换失败自动回退' })).toBeVisible()
  await expect(page.getByText('新版本通过配置校验后才会启用；启动失败就自动恢复上一个可用版本。')).toBeVisible()
  await expect(page).toHaveTitle('Sempre — 代理核心生命周期管理器')

  const dividers = await page.locator('.glass-story, .formal-band, .formal-footer').evaluateAll((elements) =>
    elements.map((element) => getComputedStyle(element, '::before').display),
  )
  expect(dividers.every((display) => display === 'none')).toBe(true)

  await page.evaluate(() => {
    document.body.classList.add('is-ready')
    document.querySelectorAll('[data-reveal]').forEach((element) => element.classList.add('is-visible'))
  })
  await page.screenshot({ path: 'test-results/homepage/zh-mobile-content.png', fullPage: true })
})

test('theme menu persists manual choices and system mode follows the OS', async ({ page }) => {
  await page.setViewportSize({ width: 360, height: 800 })
  await page.emulateMedia({ colorScheme: 'light' })
  await page.goto('/')
  await page.locator('[data-theme-trigger]').click()
  const menuBounds = await page.locator('[data-theme-menu]').boundingBox()
  expect(menuBounds).not.toBeNull()
  expect(menuBounds!.x).toBeGreaterThanOrEqual(0)
  expect(menuBounds!.x + menuBounds!.width).toBeLessThanOrEqual(360)
  await page.screenshot({ path: 'test-results/homepage/theme-menu-mobile.png' })
  await page.getByRole('menuitemradio', { name: 'Dark' }).click()
  await expect(page.locator('html')).toHaveAttribute('data-theme-preference', 'dark')
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark')
  await expect(page.locator('meta[name="theme-color"]')).toHaveAttribute('content', '#080b0a')
  expect(await page.evaluate(() => localStorage.getItem('sempre.site.theme'))).toBe('dark')
  await page.reload()
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark')

  await page.locator('[data-theme-trigger]').click()
  await page.getByRole('menuitemradio', { name: 'System' }).click()
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light')
  await page.emulateMedia({ colorScheme: 'dark' })
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark')
  await expect(page.locator('[data-theme-image]').first()).toHaveAttribute('src', '/assets/control-plane-dark.webp')
})

test('reduced motion preference disables animated transitions', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' })
  await page.goto('/')
  const duration = await page.locator('[data-copy-status]').evaluate((element) => getComputedStyle(element).transitionDuration)
  expect(Number.parseFloat(duration)).toBeLessThanOrEqual(0.01)
  await expect(page.locator('[data-reveal].is-visible').first()).toBeVisible()
})
