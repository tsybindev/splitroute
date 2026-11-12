import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  test: {
    environment: "jsdom",
    // Окружение сессии может задавать NODE_ENV=production; для React-тестов
    // нужен development-сборка (React.act), поэтому фиксируем явно.
    env: { NODE_ENV: "test" },
    setupFiles: ["./src/test/setup.ts"],
    css: false,
  },
});
