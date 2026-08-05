import { defineConfig } from 'vitest/config'

export default defineConfig({
  build: { target: 'es2022' },
  server: { host: '127.0.0.1', port: 4174, strictPort: true },
  test: { environment: 'node', include: ['src/**/*.test.ts'] },
})
