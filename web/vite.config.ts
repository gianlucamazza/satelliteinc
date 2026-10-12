import { defineConfig } from "vite";
import wasm from "vite-plugin-wasm";
import { resolve } from "path";

export default defineConfig({
  plugins: [wasm()],
  server: {
    fs: {
      allow: [".."],
    },
  },
  resolve: {
    alias: {
      "@wasm": resolve(import.meta.dirname, "../crates/wasm/pkg"),
    },
  },
  build: {
    target: "esnext",
    // Vite 8 / Rolldown: TLA is native, so vite-plugin-top-level-await
    // (require('rollup')) is gone. Rolldown only accepts a function here.
    rolldownOptions: {
      output: {
        manualChunks(id: string) {
          if (id.includes("node_modules/three")) return "three";
        },
      },
    },
  },
});
