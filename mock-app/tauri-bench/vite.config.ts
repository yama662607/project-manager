import { defineConfig } from "vite";
import path from "path";

export default defineConfig({
  build: {
    outDir: "dist-web",
    emptyOutDir: true,
    target: "safari17",
    rollupOptions: {
      input: {
        main: path.resolve(__dirname, "index.html"),
        settings: path.resolve(__dirname, "settings.html"),
      },
    },
  },
  server: {
    host: "127.0.0.1",
    strictPort: true,
    port: 5173,
  },
});
