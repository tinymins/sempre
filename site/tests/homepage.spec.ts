import { expect, test } from '@playwright/test'

const viewports = [
  { name: 'desktop', width: 1440, height: 900 },
  { name: 'compact-desktop', width: 1280, height: 720 },
  { name: 'mobile', width: 390, height: 844 },
  { name: 'small-mobile', width: 360, height: 800 },
]

for (const viewport of viewports) {
  test(`${viewport.name} layout stays within the viewport`, async ({ page }) => {
    await page.setViewportSize(viewport)
    await page.goto('/')
    await expect(page.getByRole('heading', { name: 'Sempre', level: 1 })).toBeVisible()
    await expect(page.locator('.signal-strip')).toBeVisible()
    await expect(page.locator('.product-shot img')).toHaveJSProperty('complete', true)
    const dimensions = await page.evaluate(() => ({
      viewport: document.documentElement.clientWidth,
      document: document.documentElement.scrollWidth,
      imageWidth: (document.querySelector('.product-shot img') as HTMLImageElement).naturalWidth,
    }))
    expect(dimensions.document).toBeLessThanOrEqual(dimensions.viewport)
    expect(dimensions.imageWidth).toBeGreaterThan(1000)
    await page.screenshot({ path: `test-results/${viewport.name}.png`, fullPage: true })
  })
}

test('platform command, script link, copy, and language switch together', async ({ page }) => {
  await page.goto('/')
  await page.getByRole('button', { name: 'PowerShell' }).click()
  await expect(page.locator('[data-command-output]')).toHaveText('irm https://sempre.run/install.ps1 | iex')
  await expect(page.locator('[data-script-link]')).toHaveAttribute('href', '/install.ps1')
  await page.locator('[data-copy]').click()
  await expect(page.locator('[data-copy-status]')).toHaveText('Install command copied')
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('irm https://sempre.run/install.ps1 | iex')
  await page.locator('[data-language]').click()
  await expect(page.getByRole('heading', { name: '一行命令，校验后安装。' })).toBeVisible()
  await expect(page.locator('html')).toHaveAttribute('lang', 'zh-CN')
})

test('reduced motion preference disables animated transitions', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' })
  await page.goto('/')
  const duration = await page.locator('[data-copy-status]').evaluate((element) => getComputedStyle(element).transitionDuration)
  expect(Number.parseFloat(duration)).toBeLessThanOrEqual(0.01)
})
