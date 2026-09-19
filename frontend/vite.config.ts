import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// `VITE_BASE_PATH` is `/svp/` when publishing to GitHub Pages (project site),
// and `/` when served by the svp backend binary.
export default defineConfig(({ mode }) => ({
  plugins: [react()],
  base: process.env.VITE_BASE_PATH ?? (mode === 'production' ? '/svp/' : '/'),
}))
