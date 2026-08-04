import { expect, test } from '@playwright/test'

const prototypes = [
  { name: 'spatial-glass', path: '/prototypes/spatial-glass.html' },
  { name: 'future-industrial', path: '/prototypes/future-industrial.html' },
  { name: 'obsidian-console', path: '/prototypes/obsidian-console.html' },
]

const viewports = [
  { name: 'desktop', width: 1440, height: 900 },
  { name: 'compact-desktop', width: 1280, height: 720 },
  { name: 'mobile', width: 390, height: 844 },
  { name: 'small-mobile', width: 360, height: 800 },
]

for (const prototype of prototypes) {
  for (const viewport of viewports) {
    test(`${prototype.name} ${viewport.name} layout stays within the viewport`, async ({ page }) => {
      await page.setViewportSize(viewport)
      await page.goto(prototype.path)

      await expect(page.getByRole('heading', { name: 'Sempre', level: 1 })).toBeVisible()
      await expect(page.locator('.prototype-hero')).toBeVisible()
      await expect(page.locator('[data-command-output]')).toContainText('sempre.run/install')

      const dimensions = await page.evaluate(() => {
        const images = [...document.querySelectorAll<HTMLImageElement>('img[src*="/assets/control-plane-"]')]
        return {
          viewport: document.documentElement.clientWidth,
          document: document.documentElement.scrollWidth,
          imageWidths: images.map((image) => image.naturalWidth),
          scenes: document.querySelectorAll('[data-scene]').length,
        }
      })

      expect(dimensions.document).toBeLessThanOrEqual(dimensions.viewport)
      expect(dimensions.imageWidths.length).toBeGreaterThanOrEqual(1)
      expect(dimensions.imageWidths.every((width) => width > 1000)).toBe(true)
      expect(dimensions.scenes).toBe(2)

      await page.evaluate(() => {
        document.body.classList.add('is-ready')
        document.querySelectorAll('[data-reveal]').forEach((element) => element.classList.add('is-visible'))
      })
      await page.waitForTimeout(1200)
      await page.screenshot({
        path: `test-results/prototypes/${prototype.name}-${viewport.name}.png`,
        fullPage: true,
      })
    })
  }

  test(`${prototype.name} command, locale, pointer, and scroll interactions work`, async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 })
    await page.goto(prototype.path)

    await page.getByRole('button', { name: 'PowerShell' }).click()
    await expect(page.locator('[data-command-output]')).toHaveText('irm https://sempre.run/install.ps1 | iex')
    await expect(page.locator('[data-script-link]')).toHaveAttribute('href', '/install.ps1')

    await page.locator('[data-copy]').click()
    await expect(page.locator('[data-copy-status]')).toHaveText('Install command copied')
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('irm https://sempre.run/install.ps1 | iex')

    await page.locator('[data-language]').click()
    await expect(page.locator('html')).toHaveAttribute('lang', 'zh-CN')
    await expect(page.locator('[data-language-label]')).toHaveText('EN')

    await page.mouse.move(1320, 780)
    await expect.poll(() => page.evaluate(() => Number.parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--pointer-x')))).toBeGreaterThan(0.25)

    await page.locator('[data-scene]').nth(1).scrollIntoViewIfNeeded()
    await expect.poll(() => page.evaluate(() => Number.parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--page-progress')))).toBeGreaterThan(0)
    await expect(page.locator('[data-scene]').nth(1).locator('[data-reveal].is-visible').first()).toBeVisible()
  })

  test(`${prototype.name} honors reduced motion`, async ({ page }) => {
    await page.emulateMedia({ reducedMotion: 'reduce' })
    await page.goto(prototype.path)

    const state = await page.locator('[data-reveal]').first().evaluate((element) => ({
      visible: element.classList.contains('is-visible'),
      duration: getComputedStyle(element).transitionDuration,
    }))
    expect(state.visible).toBe(true)
    expect(Number.parseFloat(state.duration)).toBeLessThanOrEqual(0.01)
  })
}
