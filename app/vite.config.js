import { defineConfig } from "vite";

// Tauri 开发模式固定连 1420 端口；dist/ 由 tauri.conf.json 的 frontendDist 打包进程序。
export default defineConfig({
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { target: "es2022", outDir: "dist", emptyOutDir: true },
});
