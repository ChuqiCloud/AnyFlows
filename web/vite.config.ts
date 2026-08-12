import { fileURLToPath, URL } from 'node:url'

import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

const apiProxyTarget = process.env.VITE_API_PROXY_TARGET?.trim() || 'http://127.0.0.1:8085'

// Vite 插件和源码别名集中在这里维护。
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  server: {
    // 本地前端保持同源请求，由 Vite 转发到独立运行的后端，避免放宽生产 CORS 边界。
    proxy: {
      '/api': apiProxyTarget,
      '/v1': apiProxyTarget,
    },
  },
})
