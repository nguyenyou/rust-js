import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import rustJs from "vite-plugin-rust-js";

// The client is the Cargo workspace's `frontend` (ADR 0101), and `/api` is
// the native server's: `cargo run -p server`, or `PILOT_API` for another.
export default defineConfig({
  plugins: [rustJs({ cargo: { package: "frontend", manifestPath: "../Cargo.toml" } }), react()],
  server: {
    proxy: { "/api": process.env.PILOT_API ?? "http://127.0.0.1:3000" },
  },
});
