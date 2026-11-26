import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  // garde les erreurs Rust visibles dans le terminal
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    watch: {
      // ne surveille PAS les sorties de build Rust : sinon Vite plante
      // avec EBUSY quand cargo ecrit les .exe des build scripts (Windows)
      ignored: ["**/src-tauri/**", "**/engine/**", "**/target/**"],
    },
  },
});
