import { defineConfig } from 'vitest/config'

export default defineConfig({
  build: { target: 'es2022' },
  server: { port: 4174 },
  test: { environment: 'node', include: ['src/**/*.test.ts'] },
})
