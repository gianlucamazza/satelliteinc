import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e",
  timeout: 60_000,
  fullyParallel: false,
  retries: 0,
  use: {
    baseURL: "http://127.0.0.1:5173",
    headless: true,
    trace: "off",
  },
  webServer: {
    // Prefer prebuilt wasm pkg; rebuild if missing.
    command:
      "sh -c 'test -f ../crates/wasm/pkg/package.json || pnpm build:wasm; pnpm exec vite --host 127.0.0.1 --port 5173'",
    url: "http://127.0.0.1:5173",
    reuseExistingServer: true,
    timeout: 120_000,
  },
});
