import { defineConfig } from "vite"
import react from "@vitejs/plugin-react"

// Tauri yêu cầu port cố định; clearScreen tắt để còn đọc được log Rust.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: "es2022",
  },
})
