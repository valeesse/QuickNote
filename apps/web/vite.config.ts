import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "path";

export default defineConfig({
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
  server: {
    proxy: {
      "/api": {
        target: "http://localhost:3000",
        changeOrigin: true,
      },
    },
  },
  build: {
    rolldownOptions: {
      output: {
        codeSplitting: {
          groups: [
            { name: "vendor-react", test: /node_modules[\\/](react|react-dom|scheduler)[\\/]/, priority: 30 },
            { name: "vendor-prosemirror", test: /node_modules[\\/](@tiptap[\\/]pm|prosemirror-)/, priority: 24 },
            { name: "vendor-tiptap-core", test: /node_modules[\\/]@tiptap[\\/](core|react|starter-kit)[\\/]/, priority: 23 },
            { name: "vendor-tiptap-extensions", test: /node_modules[\\/]@tiptap[\\/]/, priority: 22 },
          ],
        },
      },
    },
  },
});
