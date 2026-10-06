import path from 'node:path';
import { defineConfig, type Plugin } from 'vite';

// The workbench is a separate Vite app because it bundles its own Monaco build
// (@codingame/monaco-vscode-api), which cannot share a page with the panel's monaco-editor.
// It is built into frontend/public, which the panel copies into its own dist on build,
// so the page is served same-origin from /config-editor-vscode/.
const OUT_DIR = path.resolve(import.meta.dirname, '../frontend/public/config-editor-vscode');

// VS Code imports its stylesheets as modules and injects them itself, so they must arrive as strings.
function vscodeCssAsString(): Plugin {
  return {
    name: 'vscode-css-as-string',
    enforce: 'pre',
    async resolveId(source, importer, options) {
      const resolved = await this.resolve(source, importer, options);
      if (resolved && /node_modules\/@codingame\/monaco-vscode.*\.css$/.test(resolved.id)) {
        return { ...resolved, id: `${resolved.id}?inline` };
      }
      return undefined;
    },
  };
}

export default defineConfig({
  base: './',
  plugins: [vscodeCssAsString()],
  build: {
    outDir: OUT_DIR,
    emptyOutDir: true,
    target: 'esnext',
    assetsDir: 'assets',
    chunkSizeWarningLimit: 20_000,
    reportCompressedSize: false,
  },
  worker: {
    format: 'es',
  },
});
