// Draws a PDF's first pages into canvases (pdf.js, no scripts from the file).
export async function drawPdf(url: string, into: HTMLElement, pages = 1): Promise<void> {
  const pdfjs = await import("pdfjs-dist");
  // Beside the entries (see vite.config.ts); a plain string so the bundler leaves it.
  const here = import.meta.url;
  pdfjs.GlobalWorkerOptions.workerSrc = here.slice(0, here.lastIndexOf("/") + 1) + "pdf.worker.min.js";
  const doc = await pdfjs.getDocument({ url, httpHeaders: { "X-Requested-With": "ortus" }, isEvalSupported: false, enableXfa: false }).promise;
  into.replaceChildren();
  for (let n = 1; n <= Math.min(doc.numPages, pages); n++) {
    const page = await doc.getPage(n);
    const viewport = page.getViewport({ scale: 1.3 });
    const canvas = document.createElement("canvas");
    canvas.width = viewport.width;
    canvas.height = viewport.height;
    into.append(canvas);
    await page.render({ canvasContext: canvas.getContext("2d")!, viewport }).promise;
  }
}
