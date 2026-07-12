import { defineConfig } from "vite";
import wasm from "vite-plugin-wasm";
import topLevelAwait from "vite-plugin-top-level-await";
import { resolve } from "path";

export default defineConfig({
  plugins: [wasm(), topLevelAwait()],
  server: {
    fs: {
      allow: [".."],
    },
  },
  resolve: {
    alias: {
      "@wasm": resolve(__dirname, "../crates/wasm/pkg"),
    },
  },
  build: {
    target: "esnext",
    rollupOptions: {
      // three.js only changes on upgrades: keep it in its own chunk so
      // returning players re-download just the app code.
      output: { manualChunks: { three: ["three"] } },
    },
  },
});
