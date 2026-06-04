/// <reference types="vitest" />
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// Vitest config keeps the existing Vite plugin pipeline (React + proxy
// mappings) so that test files run inside the same module graph that
// `vite build` uses for production output.
export default defineConfig({
  plugins: [react()],
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/test/setup.ts'],
    css: false,
  },
})
