import path from 'path'
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  server: {
    proxy: {
      '/api': {
        target: 'http://localhost:8080',
        changeOrigin: true,
        ws: true,
      },
      '/static': {
        target: 'http://localhost:8080',
        changeOrigin: true,
      },
      '/skill.md': {
        target: 'http://localhost:8080',
        changeOrigin: true,
      },
    },
  },
})
