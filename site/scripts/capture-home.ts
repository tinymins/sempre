import { chromium } from '@playwright/test'
import { mkdir } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const siteRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const output = resolve(siteRoot, 'public/assets/og-home.png')
const siteURL = process.env.SEMPRE_SITE_URL || 'http://127.0.0.1:4174'

await mkdir(dirname(output), { recursive: true })
const browser = await chromium.launch()
try {
  const page = await browser.newPage({ viewport: { width: 1200, height: 630 }, deviceScaleFactor: 1 })
  await page.goto(siteURL, { waitUntil: 'networkidle' })
  await page.screenshot({ path: output, type: 'png' })
  console.log(output)
} finally {
  await browser.close()
}
