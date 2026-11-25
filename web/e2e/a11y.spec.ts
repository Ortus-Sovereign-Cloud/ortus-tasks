// WCAG 2.2 AA with axe on this app's screens: desktop and phone, light
// and dark. The page's CSP forbids inline scripts; the test bypasses it
// to put axe in.
import { expect, test } from "@playwright/test";
import { axe } from "./axe.js";

test.use({ bypassCSP: true });

const PAGES = ["/tasks", "/tasks?view=mine", "/records", "/approvals"];

for (const [name, size] of [["desktop", { width: 1440, height: 900 }], ["phone", { width: 390, height: 844 }]] as const) {
  for (const scheme of ["light", "dark"] as const) {
    test(`the tasks screens pass axe: ${name}, ${scheme}`, async ({ page }) => {
      await page.setViewportSize(size);
      await page.emulateMedia({ colorScheme: scheme });
      const failures: string[] = [];
      for (const path of PAGES) {
        await page.goto(path);
        // Live streams keep the network busy: wait for the screen instead.
        await page.locator("main#content > *").first().waitFor();
        await page.waitForTimeout(1000);
        failures.push(...(await axe(page)).map((f) => `${path}: ${f}`));
      }
      expect(failures).toEqual([]);
    });
  }
}
