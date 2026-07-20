import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "path";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig(async () => ({
  plugins: [react()],
  resolve: {
    dedupe: ["prosemirror-model", "prosemirror-state", "prosemirror-view", "yjs", "y-protocols"],
    alias: {
      "@": path.resolve(__dirname, "./src"),
      "@ui": path.resolve(__dirname, "../../packages/ui/src"),
      "@contracts": path.resolve(__dirname, "../../packages/contracts/src"),
      "react": path.resolve(__dirname, "node_modules/react"),
      "react-dom": path.resolve(__dirname, "node_modules/react-dom"),
      "lucide-react": path.resolve(__dirname, "node_modules/lucide-react"),
      "yjs": path.resolve(__dirname, "node_modules/yjs"),
      "y-indexeddb": path.resolve(__dirname, "node_modules/y-indexeddb"),
      "y-protocols": path.resolve(__dirname, "node_modules/y-protocols"),
      "@tiptap/core": path.resolve(__dirname, "node_modules/@tiptap/core"),
      "@tiptap/pm": path.resolve(__dirname, "node_modules/@tiptap/pm"),
      "@tiptap": path.resolve(__dirname, "node_modules/@tiptap"),
    },
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    rolldownOptions: {
      output: {
        codeSplitting: {
          groups: [
            {
              name: "vendor-react",
              test: /node_modules[\\/](react|react-dom|scheduler)[\\/]/,
              priority: 40,
            },
            {
              name: "vendor-prosemirror",
              test: /node_modules[\\/](@tiptap[\\/]pm|prosemirror-)/,
              priority: 34,
            },
            {
              name: "vendor-tiptap-core",
              test: /node_modules[\\/]@tiptap[\\/](core|react|starter-kit)[\\/]/,
              priority: 33,
            },
            {
              name: "vendor-tiptap-extensions",
              test: /node_modules[\\/]@tiptap[\\/]/,
              priority: 32,
            },
            {
              name: "vendor-lowlight",
              test: /node_modules[\\/](lowlight|highlight\.js)[\\/]/,
              priority: 20,
            },
            {
              name: "vendor-crdt",
              test: /node_modules[\\/](yjs|y-indexeddb|lib0)[\\/]/,
              priority: 10,
            },
          ],
        },
      },
    },
  },
}));
