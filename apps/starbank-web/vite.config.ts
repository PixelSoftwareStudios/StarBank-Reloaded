import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";
import tauriConfig from "../starbank-desktop/src-tauri/tauri.conf.json" with {
  type: "json",
};

export default defineConfig({
  base: "./",
  define: {
    __STARBANK_PLATFORM_INFO__: JSON.stringify({
      version: tauriConfig.version,
      platform: "web",
    }),
  },
  build: { target: ["chrome111", "edge111", "firefox128", "safari16.4"] },
  plugins: [react(), tailwindcss()],
});
