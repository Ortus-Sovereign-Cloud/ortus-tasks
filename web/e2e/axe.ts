// axe-core (WCAG 2.0–2.2, A and AA) on a page, inside shadow roots too.
// Test-only: axe-core is MPL-2.0 and never ships in the program.
import { createRequire } from "node:module";
import type { Page } from "@playwright/test";

const AXE = createRequire(import.meta.url).resolve("axe-core/axe.min.js");
const TAGS = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"];

/** The page's WCAG failures, one line each (empty when it passes). */
export async function axe(page: Page): Promise<string[]> {
  await page.addScriptTag({ path: AXE });
  return page.evaluate(async (tags) => {
    const run = (window as unknown as { axe: { run: (c: Document, o: object) => Promise<{ violations: { id: string; help: string; nodes: { target: unknown[] }[] }[] }> } }).axe.run;
    const { violations } = await run(document, { runOnly: { type: "tag", values: tags } });
    return violations.map((v) => `${v.id}: ${v.help} (${v.nodes.map((n) => JSON.stringify(n.target)).slice(0, 3).join(", ")})`);
  }, TAGS);
}
