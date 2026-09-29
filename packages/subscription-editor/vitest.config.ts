import { defineConfig } from 'vitest/config'
export default defineConfig({ test: { environment: 'jsdom', setupFiles: '../components/src/test/setup.ts' } })
