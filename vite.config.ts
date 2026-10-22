import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "node:path";

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react(), tailwindcss()],
  // StageCue frontend must run fully offline; no CDN assets allowed.
  build: {
    target: "es2022",
    rollupOptions: {
      input: {
        // Two entries: operator console, and the Stage Output window content.
        console: path.resolve(__dirname, "index.html"),
        output: path.resolve(__dirname, "output.html"),
      },
    },
  },
});
