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
      { find: '@acme/types', replacement: fileURLToPath(new URL('./src/features/subscriptions/toolbox/types/proxy.ts', import.meta.url)) },
      { find: 'react-i18next', replacement: fileURLToPath(new URL('./src/features/subscriptions/toolbox/i18n-compat.ts', import.meta.url)) },
      { find: '@', replacement: fileURLToPath(new URL('./src', import.meta.url)) },
    ],
  },
  server: {
    port: 5173,
    proxy: {
      '/api': 'http://127.0.0.1:33211',
    },
  },
  build: {
    target: 'es2022',
  },
  test: {
    environment: 'jsdom',
    setupFiles: './src/test/setup.ts',
    alias: {
      'monaco-editor': fileURLToPath(new URL('./src/test/monaco-runtime.ts', import.meta.url)),
    },
  },
})
