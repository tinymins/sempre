import { defineConfig } from '@playwright/test'

const port = Number(process.env.SEMPRE_SITE_PORT || 4174)
const baseURL = `http://127.0.0.1:${port}`

export default defineConfig({
  testDir: './tests',
  outputDir: './test-results',
  use: {
    baseURL,
    permissions: ['clipboard-read', 'clipboard-write'],
    screenshot: 'only-on-failure',
  },
  webServer: {
    command: `bun run dev -- --host 127.0.0.1 --port ${port}`,
    url: baseURL,
    reuseExistingServer: true,
  },
})
