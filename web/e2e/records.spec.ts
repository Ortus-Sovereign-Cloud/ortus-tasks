// Records (WebRecordFiled) and read-and-confirm (WebMustRead).
import { expect, test } from "@playwright/test";
import en from "../src/locales/en.js";

const headers = { "X-Requested-With": "ortus" };

test("a filed record: its page, its signatures checked", async ({ page }) => {
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
  await confirm.click();
  await expect(read.getByRole("article")).toContainText("Summary for new staff.");
  await read.getByRole("checkbox", { name: en.readAndUnderstood }).check();
  await read.getByRole("button", { name: en.confirm, exact: true }).click();
  await expect(read.getByRole("status")).toContainText(en.allConfirmed);
  const welcome = await (await page.request.get("/api/v1/me/welcome", { headers })).json();
  expect(welcome.steps.find((s: { id: string }) => s.id === "policies").done).toBe(true);
});
