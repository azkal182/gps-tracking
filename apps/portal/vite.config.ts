import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { loadEnv } from 'vite'
import { defineConfig } from 'vitest/config'

// https://vite.dev/config/
export default defineConfig(({ mode }) => {
  const projectRoot = path.dirname(fileURLToPath(import.meta.url))
  const env = loadEnv(mode, projectRoot, 'VITE_')
  const apiTarget = env.VITE_API_PROXY_TARGET || 'http://127.0.0.1:8080'

  return {
    plugins: [react(), tailwindcss()],
    resolve: {
      alias: { '@': path.resolve(projectRoot, './src') },
    },
    server: {
      proxy: {
        // API failures must remain API responses and never fall back to SPA HTML.
        '/v1': { target: apiTarget, changeOrigin: true, ws: true },
      },
    },
    test: {
      environment: 'jsdom',
      setupFiles: './src/test/setup.ts',
      css: true,
      exclude: [
        'e2e/**',
        '**/node_modules/**',
        '**/dist/**',
        'test-results/**',
      ],
    },
  }
})
