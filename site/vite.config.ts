import { defineConfig } from 'vitest/config'

export default defineConfig({
  build: {
    target: 'es2022',
    rollupOptions: {
      input: [
        'index.html',
        'prototypes/spatial-glass.html',
        'prototypes/future-industrial.html',
        'prototypes/obsidian-console.html',
      ],
    },
  },
  server: { port: 4174 },
  test: { environment: 'node', include: ['src/**/*.test.ts'] },
})
