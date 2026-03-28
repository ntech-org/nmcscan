import tailwindcss from "@tailwindcss/vite";
import { sveltekit } from "@sveltejs/kit/vite";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [tailwindcss(), sveltekit()],
  server: {
    port: 5173,
    proxy: {
      "/api": {
        target: "http://localhost:3000",
        changeOrigin: true,
        autoRewrite: true,
      },
    },
  },
  build: {
    // Suppress warning about "this" keyword in CJS modules from @auth/core
    rollupOptions: {
      onwarn(warning, warn) {
        if (warning.code === 'THIS_IS_UNDEFINED') return;
        warn(warning);
      }
    },
    // Performance: Disabling sourcemaps and assets inlining for faster build
    sourcemap: false,
    assetsInlineLimit: 0,
  }
});
