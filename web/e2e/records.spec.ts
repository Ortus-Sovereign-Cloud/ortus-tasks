// Records (WebRecordFiled) and read-and-confirm (WebMustRead).
import { expect, test } from "@playwright/test";
import en from "../src/locales/en.js";

const headers = { "X-Requested-With": "ortus" };

test("a filed record: its page, its signatures checked; on a phone the list, then the record with a back button", async ({ page }) => {
  const filed = await (await page.request.post("/api/v1/records/dev-sample", { headers })).json();
  await page.goto("/records");
  const home = page.locator("ortus-records-home");
  await expect(home.getByRole("table")).toContainText("0114-");
  await home.getByRole("link", { name: "Flood response plan 2026" }).first().click();
  const record = page.locator("ortus-records-record");
  await expect(record.getByRole("status")).toContainText("Signed and filed at");
  const facts = record.getByRole("region", { name: en.record, exact: true });
  await expect(facts).toContainText("2026/EM/0114".split("/").pop()!);
  await expect(facts).toContainText("1 of 1 valid");
  await expect(facts).toContainText("10 years");
  await expect(record.getByRole("region", { name: en.whatNext })).toContainText("Filed to 2026/EM/0114");
  await expect(record.locator(".seal")).toHaveText(en.sealed);
  await expect(record.locator("canvas")).toHaveCount(1);
  await record.getByRole("button", { name: en.verifySignatures }).click();
  await expect(record.getByRole("region", { name: en.signatures })).toContainText(en.checkValid);
  await expect(record.getByRole("region", { name: en.signatures })).toContainText(en.fileUnchanged);
  await page.screenshot({ path: "test-results/record.png" });
  const pdf = await page.request.get(`/api/v1/records/records/${filed.id}/pdf`, { headers });
  expect((await pdf.body()).subarray(0, 5).toString()).toBe("%PDF-");

  // On a phone: the list, then a record full width with a back button.
  await page.setViewportSize({ width: 390, height: 844 });
  const fits = async () => expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
  await page.goto("/records");
  const list = page.locator("ortus-records-home");
  await expect(list.getByRole("table")).toContainText("0114-");
  await fits();
  await page.screenshot({ path: "test-results/phone-records.png" });
  await list.getByRole("link", { name: "Flood response plan 2026" }).first().click();
  await expect(record.getByRole("region", { name: en.record, exact: true })).toContainText("10 years");
  await expect(record.locator("canvas")).toHaveCount(1);
  const app = (await record.boundingBox())!;
  expect((await record.getByRole("region", { name: en.record, exact: true }).boundingBox())!.width).toBeGreaterThan(app.width - 30);
  await fits();
  await page.screenshot({ path: "test-results/phone-record.png", fullPage: true });
  await record.getByRole("button", { name: en.back }).click();
  await expect(page.locator("ortus-records-home").getByRole("table")).toBeVisible();
});

test("must-read policies are read and confirmed one by one", async ({ page }) => {
  const body = (text: string) => ({ type: "doc", content: [{ type: "paragraph", content: [{ type: "text", text: "Three rules cover most of your day." }] }, { type: "paragraph", content: [{ type: "text", marks: [{ type: "strong" }], text }] }] });
  for (const [title, text] of [["Data protection policy", "1. Look at the label."], ["Flood response plan 2026", "Summary for new staff."]]) {
    const r = await page.request.post("/api/v1/records/must-read", { headers, data: { title, note: "Handbook · owner: Data protection officer", body: body(text!) } });
    expect(r.ok()).toBeTruthy();
  }
  await page.goto("/records?read=1");
  const read = page.locator("ortus-records-must-read");
  await expect(read.getByRole("article")).toContainText("Data protection policy");
  await expect(read.getByRole("article")).toContainText("Look at the label.");
  await expect(read.getByText("Policy 1 of 2")).toBeVisible();
  const confirm = read.getByRole("button", { name: en.confirmNext });
  await expect(confirm).toBeDisabled();
  await read.getByRole("checkbox", { name: en.readAndUnderstood }).check();
  await page.screenshot({ path: "test-results/must-read.png" });
  // On a phone: one column, nothing wider than the screen.
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
  await page.screenshot({ path: "test-results/phone-must-read.png", fullPage: true });
  await page.setViewportSize({ width: 1440, height: 900 });
  await confirm.click();
  await expect(read.getByRole("article")).toContainText("Summary for new staff.");
  await read.getByRole("checkbox", { name: en.readAndUnderstood }).check();
  await read.getByRole("button", { name: en.confirm, exact: true }).click();
  await expect(read.getByRole("status")).toContainText(en.allConfirmed);
  const welcome = await (await page.request.get("/api/v1/me/welcome", { headers })).json();
  expect(welcome.steps.find((s: { id: string }) => s.id === "policies").done).toBe(true);
});
