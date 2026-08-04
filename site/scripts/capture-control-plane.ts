import { chromium } from '@playwright/test'
import sharp from 'sharp'
import { mkdir } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const siteRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const uiURL = process.env.SEMPRE_UI_URL || 'http://127.0.0.1:5175'
const themes = ['light', 'dark'] as const

const system = {
  version: 'v0.1.0', commit: '8a29f0c', date: '2026-08-04T12:00:00Z', mode: 'system', service: 'running',
  desired_state: 'running', runtime: { state: 'running', pid: 12842, restart_count: 0 },
  selected: { core: 'sing-box', ref: 'stable' },
  active: { core: 'sing-box', ref: 'stable', version: '1.13.15', config_hash: '4df9e8' },
  pending: false, web: { listen: '127.0.0.1:33211', local_url: 'http://127.0.0.1:33211', password_set: true, password_warning: false },
  ui: { installed: true }, capabilities: {},
}

const runtime = {
  desired_state: 'running', runtime_state: 'running',
  active: { core: 'sing-box', ref: 'stable', version: '1.13.15', exact_reference: 'sing-box@1.13.15', config_hash: '4df9e8' },
  target: { core: 'sing-box', ref: 'stable', version: '1.13.15', exact_reference: 'sing-box@1.13.15', config_hash: '4df9e8' },
  pid: 12842, started_at: '2026-08-04T11:12:00Z', uptime_seconds: 13142, restart_count: 0, pending: false,
  last_transition: '2026-08-04T11:12:00Z',
  actions: { start: { allowed: false }, stop: { allowed: true }, restart: { allowed: true } },
}

await mkdir(resolve(siteRoot, 'public/assets'), { recursive: true })
const browser = await chromium.launch()
try {
  for (const theme of themes) {
    const context = await browser.newContext({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1 })
    const page = await context.newPage()
    await page.addInitScript(({ baseURL, selectedTheme }) => {
      sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL, token: 'demo', expiresAt: '2099-01-01T00:00:00Z' }))
      localStorage.setItem('sempre.locale', 'en')
      localStorage.setItem('sempre.theme', selectedTheme)
    }, { baseURL: uiURL, selectedTheme: theme })
    await page.route('**/api/v1/**', async (route) => {
      const path = new URL(route.request().url()).pathname
      if (path.endsWith('/runtime/events')) {
        await route.fulfill({
          status: 200,
          contentType: 'text/event-stream',
          body: [
            'data: {"topic":"traffic","data":{"down":5862400,"up":824300}}\n\n',
            'data: {"topic":"memory","data":{"inuse":91226112}}\n\n',
            'data: {"topic":"connections","data":{"connections":[1,2,3,4,5,6,7,8,9,10,11,12]}}\n\n',
          ].join(''),
        })
        return
      }
      if (path.endsWith('/runtime/status')) {
        await route.fulfill({ json: runtime })
        return
      }
      if (path.endsWith('/runtime/overview')) {
        await route.fulfill({ json: { core: 'sing-box', version: '1.13.15', mode: 'rule', connections: 12, download: 8589934592, upload: 1288490188 } })
        return
      }
      if (path.endsWith('/system')) {
        await route.fulfill({ json: system })
        return
      }
      await route.fulfill({ status: 404, json: { error: { code: 'NOT_FOUND', message: 'Not found' } } })
    })
    await page.goto(uiURL, { waitUntil: 'networkidle' })
    await page.getByRole('heading', { name: 'Overview' }).waitFor()
    await page.waitForFunction((selectedTheme) => document.documentElement.classList.contains('dark') === (selectedTheme === 'dark'), theme)
    await page.waitForTimeout(3500)
    const png = await page.screenshot({ type: 'png', clip: { x: 0, y: 0, width: 1600, height: 860 } })
    const output = resolve(siteRoot, `public/assets/control-plane-${theme}.webp`)
    await sharp(png).webp({ quality: 88 }).toFile(output)
    console.log(output)
    await context.close()
  }
} finally {
  await browser.close()
}
