import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'
import rustJs from 'vite-plugin-rust-js'

// https://vite.dev/config/
export default defineConfig({
  // rust-js compiles src/App.rs to src/App.jsx, which plugin-react serves
  // with Fast Refresh.
  plugins: [rustJs(), react()],
})
