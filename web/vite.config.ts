import { copyFileSync } from "node:fs";
import { defineConfig } from "vite";

// The client of this crate's three apps, embedded in the crate and served
// by the core under /assets/apps/<app>/: each app has its own entry
// (app.js for tasks, approvals.js, records.js; the crate serves each as
// that app's app.js). "ortus-kit" stays outside the bundle: the browser
// loads the core's one copy from /assets/kit.js.
export default defineConfig({
  plugins: [
    {
      // pdf.js draws PDFs in a worker of its own, served beside the entries.
      name: "pdf-worker",
      closeBundle() {
        copyFileSync("node_modules/pdfjs-dist/build/pdf.worker.min.mjs", "dist/pdf.worker.min.js");
      },
    },
  ],
  build: {
    outDir: "dist",
    emptyOutDir: false,
    sourcemap: false,
    target: "es2022",
    lib: { entry: { app: "src/app.ts", approvals: "src/approvals-app.ts", records: "src/records-app.ts" }, formats: ["es"], fileName: (_, name) => `${name}.js` },
    rollupOptions: {
      external: ["ortus-kit"],
      output: { paths: { "ortus-kit": "/assets/kit.js" }, chunkFileNames: "[name]-[hash].js" },
    },
  },
});
