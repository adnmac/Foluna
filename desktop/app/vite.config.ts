import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Serveur fixe pour `tauri dev` (devUrl de tauri.conf.json) ; l'interface est dans `ui/`.
export default defineConfig({
  root: "ui",
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    outDir: "../dist",
    emptyOutDir: true,
    target: "es2022",
  },
});
