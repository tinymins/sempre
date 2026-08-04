import { defineConfig } from 'vitest/config'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import { fileURLToPath, URL } from 'node:url'

export default defineConfig({
  base: './',
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: [
      { find: '@acme/components/icons', replacement: fileURLToPath(new URL('./src/components/acme/icons/index.tsx', import.meta.url)) },
      { find: '@acme/components', replacement: fileURLToPath(new URL('./src/components/acme/index.ts', import.meta.url)) },
    ],
  },
  server: {
    port: 5173,
  },
  build: {
    target: 'es2022',
  },
  test: {
    environment: 'jsdom',
    setupFiles: './src/test/setup.ts',
  },
})
