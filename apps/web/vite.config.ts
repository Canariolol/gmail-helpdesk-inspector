import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  // Las variables VITE_* viven en el .env de la raíz del repo (junto a las del API).
  envDir: "../..",
  server: {
    port: 5173,
  },
});

