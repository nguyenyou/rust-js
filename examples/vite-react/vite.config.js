import babel from '@rolldown/plugin-babel'
import tailwindcss from '@tailwindcss/vite'
import react, { reactCompilerPreset } from '@vitejs/plugin-react'
import { defineConfig } from 'vite'
import rustJs from '@rust-js/vite-plugin'

// https://vite.dev/config/
export default defineConfig({
  plugins: [
    // rust-js compiles the app's crate, src/App.rs, with Cargo (ADR 0101),
    // to src/App.jsx, which plugin-react serves with Fast Refresh, and React
    // Compiler memoizes.
    rustJs({ cargo: { package: 'app' } }),
    react(),
    babel({ presets: [reactCompilerPreset()] }),
    tailwindcss(),
  ],
})
