/// <reference types="vitest/config" />
import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";

const host = process.env.TAURI_DEV_HOST;

// The shell's build script reads this file and rebuilds a bundle of the other
// flavour, so an e2e bundle is never embedded in a release (bundle.rs).
const bundleFlavour: Plugin = {
  name: "bundle-flavour",
  generateBundle() {
    this.emitFile({
      type: "asset",
      fileName: "bundle-flavour",
      source: process.env.VITE_E2E ? "e2e" : "release",
    });
  },
};

export default defineConfig({
  plugins: [react(), bundleFlavour],
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
