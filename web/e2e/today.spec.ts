// The start page's cards from this repository: the tasks card (one's open
// tasks, a late one marked) and the approvals card.
import { expect, test } from "@playwright/test";
import en from "../src/locales/en.js";

const headers = { "X-Requested-With": "ortus" };

async function card(page: import("@playwright/test").Page, app: string): Promise<void> {
  await page.evaluate(async (app) => {
    const file = `/assets/apps/${app}/today.js`;
    await import(/* @vite-ignore */ file);
    document.body.append(document.createElement(`ortus-${app}-today`));
  }, app);
}

/** A person of this test's own (the owner's own tasks are counted by other tests). */
async function newcomer(owner: import("@playwright/test").Page, browser: import("@playwright/test").Browser): Promise<import("@playwright/test").Page> {
  const email = `today-${Date.now()}-${Math.floor(Math.random() * 1e6)}@ministry.example`;
  const invited = await owner.request.post("/api/v1/admin/users", { headers, data: { email, name: "Marek Nowak" } });
  expect(invited.status(), await invited.text()).toBe(200);
  const link = new URL(((await invited.json()) as { link: string }).link);
  const page = await (await browser.newContext({ storageState: { cookies: [], origins: [] } })).newPage();
  await page.goto(link.pathname + link.hash);
  await expect(page).toHaveURL(/\/welcome$/);
  return page;
}

test("the tasks card lists one's open tasks and marks a late one", async ({ page: owner, browser }) => {
  const page = await newcomer(owner, browser);
  const me = (await (await page.request.get("/api/v1/me")).json()) as { email: string };
  const project = (await (await page.request.post("/api/v1/tasks/projects", { headers, data: { name: `Flood ${Date.now()}`, members: [] } })).json()) as { id: string };
  const title = `Check the pumps ${Date.now()}`;
  const made = await page.request.post("/api/v1/tasks/tasks", { headers, data: { project: project.id, title, notes: "", assignee: me.email, dueAt: Date.now() - 3_600_000, state: "todo" } });
  expect(made.status(), await made.text()).toBe(200);
  const task = (await made.json()) as { id: string };
  await page.goto("/tasks");
  await card(page, "tasks");
  const section = page.locator("ortus-tasks-today").getByRole("region", { name: en.tasks });
  const row = section.getByRole("link").filter({ hasText: title });
  await expect(row).toHaveAttribute("href", `/tasks?project=${project.id}&task=${task.id}`);
  await expect(row.locator(".late")).toBeVisible();
  await expect(section.getByRole("link", { name: en.todayOpenTasks })).toHaveAttribute("href", "/tasks?view=mine");
});

test("the approvals card says what waits, or that nothing does", async ({ page }) => {
  await page.goto("/approvals");
  await card(page, "approvals");
  const section = page.locator("ortus-approvals-today").getByRole("region", { name: en.waitingForYou });
  await expect(section.getByRole("link", { name: en.todayOpenApprovals })).toHaveAttribute("href", "/approvals");
  // Without the docs app here, nothing waits.
  const answer = (await (await page.request.get("/api/v1/docs/reviews/mine")).json().catch(() => ({}))) as { waiting?: unknown[] };
  const waiting = answer.waiting ?? [];
  if (!waiting.length) await expect(section).toContainText(en.nothingWaiting);
});
