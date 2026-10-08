import { expect, test } from '@playwright/test'

test('install options generate quoted POSIX and PowerShell commands', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 })
  await page.goto('/')
  await page.getByLabel('Core (optional)').fill('sing-box:tinymins/sing-box@13.11.2')
  await page.getByLabel('Subscription URL (optional)').fill("https://domain.com/subscription/user's-token")
  await page.getByLabel('Web UI').selectOption('github')
  await page.getByLabel('UI source').fill('tinymins/sempre-ui@stable')

  const posix = "curl -fsSL https://sempre.run/install | sh -s -- --core='sing-box:tinymins/sing-box@13.11.2' --subscription='https://domain.com/subscription/user'\"'\"'s-token' --ui='tinymins/sempre-ui@stable'"
  await expect(page.locator('[data-command-output]')).toHaveText(posix)
  await page.locator('[data-copy]').click()
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(posix)

  await page.getByRole('button', { name: 'PowerShell' }).click()
  const powershell = "& ([scriptblock]::Create((irm https://sempre.run/install.ps1))) -Core 'sing-box:tinymins/sing-box@13.11.2' -Subscription 'https://domain.com/subscription/user''s-token' -UI 'tinymins/sempre-ui@stable'"
  await expect(page.locator('[data-command-output]')).toHaveText(powershell)
})

test('HTTPS UI exposes its checksum option and subscription input is never persisted', async ({ page }) => {
  await page.goto('/')
  await page.getByLabel('Subscription URL (optional)').fill('https://domain.com/private-token')
  await page.getByLabel('Web UI').selectOption('url')
  await expect(page.getByLabel('UI source')).toHaveAttribute('placeholder', 'https://example.com/sempre-ui.zip')
  await page.getByLabel('UI source').fill('https://example.com/custom-ui.zip')
  await page.getByLabel('UI SHA-256').fill('a'.repeat(64))
  await expect(page.locator('[data-command-output]')).toContainText("--ui-sha256='" + 'a'.repeat(64) + "'")

  const storageBeforeReload = await page.evaluate(() => ({ ...localStorage }))
  expect(Object.values(storageBeforeReload)).not.toContain('https://domain.com/private-token')
  await page.reload()
  await expect(page.getByLabel('Subscription URL (optional)')).toHaveValue('')
  await expect(page.locator('[data-command-output]')).toHaveText('curl -fsSL https://sempre.run/install | sh')
})
