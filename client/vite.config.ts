import { execSync } from "node:child_process";
import path from "node:path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import wasm from "vite-plugin-wasm";
import topLevelAwait from "vite-plugin-top-level-await";
import { VitePWA } from "vite-plugin-pwa";
import { compression } from "vite-plugin-compression2";

function gitHash(): string {
  try {
    return execSync("git rev-parse --short HEAD").toString().trim();
  } catch {
    return "dev";
  }
}

export default defineConfig({
  resolve: {
    alias: {
      "@wasm/engine": path.resolve(__dirname, "src/wasm/engine_wasm"),
    },
  },
  plugins: [
    react(),
    tailwindcss(),
    wasm(),
    topLevelAwait(),
    VitePWA({
      registerType: "autoUpdate",
      manifest: false, // Use public/manifest.json
      includeAssets: ["**/*.mp3", "**/*.m4a"],
      workbox: {
        maximumFileSizeToCacheInBytes: 15 * 1024 * 1024,
        runtimeCaching: [
          {
            urlPattern: /card-data\.json$/,
            handler: "CacheFirst",
            options: {
              cacheName: "card-database",
              expiration: { maxEntries: 1, maxAgeSeconds: 604800 },
            },
          },
          {
            urlPattern: /^https:\/\/api\.scryfall\.com\//,
            handler: "NetworkFirst",
            options: {
              cacheName: "scryfall-api",
              expiration: { maxEntries: 500, maxAgeSeconds: 86400 },
            },
          },
          {
            urlPattern: /^https:\/\/cards\.scryfall\.io\//,
            handler: "CacheFirst",
            options: {
              cacheName: "scryfall-images",
              expiration: { maxEntries: 2000, maxAgeSeconds: 604800 },
            },
          },
          {
            urlPattern: /^https:\/\/pub-fc5b5c2c6e774356ae3e730bb0326394\.r2\.dev\/audio\//,
            handler: "CacheFirst",
            options: {
              cacheName: "audio-r2",
              expiration: { maxEntries: 50, maxAgeSeconds: 2592000 },
            },
          },
        ],
      },
    }),
    compression({ algorithms: ["brotliCompress"] }),
  ],
  define: {
    __APP_VERSION__: JSON.stringify(process.env.npm_package_version ?? "0.1.0"),
    __BUILD_HASH__: JSON.stringify(gitHash()),
    __CARD_DATA_URL__: JSON.stringify(
      process.env.CARD_DATA_URL || "/card-data.json",
    ),
    __COVERAGE_DATA_URL__: JSON.stringify(
      process.env.COVERAGE_DATA_URL || "/coverage-data.json",
    ),
    __COVERAGE_SUMMARY_URL__: JSON.stringify(
      process.env.COVERAGE_SUMMARY_URL || "/coverage-summary.json",
    ),
    __AUDIO_BASE_URL__: JSON.stringify(
      process.env.AUDIO_BASE_URL || "",
    ),
  },
  build: {
    target: "esnext",
  },
});
