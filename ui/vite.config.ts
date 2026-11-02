/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/setup-tests.ts"],
    exclude: ["e2e/**", "node_modules/**"],
    // The onboarding matrix renders every state in one file, so one worker
    // holds the whole sweep; node's default heap no longer fits it.
    poolOptions: { forks: { execArgv: ["--max-old-space-size=8192"] } },
  },
});
