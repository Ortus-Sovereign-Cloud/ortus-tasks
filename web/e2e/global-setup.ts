// Creates the first owner through the real setup page, once, and keeps
// the signed-in session for every test (test-results/owner.json).
import { chromium, expect, type FullConfig } from "@playwright/test";

export default async function globalSetup(config: FullConfig): Promise<void> {
  const baseURL = config.projects[0]?.use.baseURL as string;
  const browser = await chromium.launch();
  const page = await browser.newPage({ baseURL });
  await page.goto("/setup#e2e-setup-token");
  await page.getByLabel("Your name").fill("Olena Hrytsenko");
  await page.getByLabel("Work email").fill("olena@ministry.example");
  await page.getByRole("button", { name: "Create the owner" }).click();
  await expect(page.locator("ortus-shell")).toBeAttached();
  await page.context().storageState({ path: "test-results/owner.json" });

  await browser.close();
}
