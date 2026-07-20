import path from 'path'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import { defineConfig } from 'vitest/config'

// https://vitejs.dev/config/
export default defineConfig({
    plugins: [react(), tailwindcss()],

    test: {
        environment: 'jsdom',
        setupFiles: ['./src/test-setup.ts'],
        exclude: ['**/node_modules/**', '**/src-tauri/**']
    },

    // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
    //
    // 1. prevent vite from obscuring rust errors
    clearScreen: false,
    // 2. tauri expects a fixed port, fail if that port is not available
    server: {
        port: 1420,
        strictPort: true,
        watch: {
            // 3. tell vite to ignore watching `src-tauri`
            ignored: ['**/src-tauri/**']
        }
    },

    // Shadcn UI
    resolve: {
        alias: {
            '@': path.resolve(__dirname, './src')
        }
    }
})
