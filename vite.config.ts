import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  server: {
    host: '127.0.0.1',
    port: 1420,
    strictPort: true,
    watch: {
      ignored: [
        '**/src-tauri/**',
        '**/benchmarks/generated/**',
        '**/test-results/**',
      ],
    },
  },
  clearScreen: false,
  build: { target: 'es2022' },
});
