import babel from '@rolldown/plugin-babel'
import tailwindcss from '@tailwindcss/vite'
import react, { reactCompilerPreset } from '@vitejs/plugin-react'
import { defineConfig } from 'vite'
import rustJs from 'vite-plugin-rust-js'

// https://vite.dev/config/
export default defineConfig({
  plugins: [
    // rust-js compiles src/App.rs to src/App.jsx, which plugin-react serves
    // with Fast Refresh, and React Compiler memoizes.
    rustJs(),
    react(),
    babel({ presets: [reactCompilerPreset()] }),
    tailwindcss(),
  ],
})
