import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";
import tauriConfig from "./src-tauri/tauri.conf.json" with { type: "json" };

function desktopPlatform(target: string) {
  switch (target) {
    case "darwin":
    case "macos":
      return "macos";
    case "linux":
      return "linux";
    case "windows":
    case "win32":
      return "windows";
    default:
      return "desktop";
  }
}

export default defineConfig({
  base: "./",
  define: {
    __STARBANK_PLATFORM_INFO__: JSON.stringify({
      version: tauriConfig.version,
      platform: desktopPlatform(
        process.env.TAURI_ENV_PLATFORM ?? process.platform,
      ),
    }),
  },
  build: { target: ["chrome111", "edge111", "firefox128", "safari16.4"] },
  plugins: [react(), tailwindcss()],
  server: { host: "127.0.0.1", port: 1420, strictPort: true },
});
