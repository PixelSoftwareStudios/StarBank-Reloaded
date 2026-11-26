import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  base: "./",
  build: { target: ["chrome111", "edge111", "firefox128", "safari16.4"] },
  plugins: [react(), tailwindcss()],
  server: { host: "127.0.0.1", port: 1420, strictPort: true },
});
