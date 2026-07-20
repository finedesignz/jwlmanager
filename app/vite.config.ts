import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri v2 dev server contract: fixed port, no clearScreen so Rust build errors
// stay visible, HMR over a fixed port for the webview.
export default defineConfig(async () => ({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
}));
