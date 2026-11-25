// The board (WebTasks): a project, tasks with who and when, a card moved
// to another column, a checklist ticked, the list, the timeline and
// one's own tasks.
import { expect, test } from "@playwright/test";
import en from "../src/locales/en.js";

test.describe.configure({ mode: "serial" });

test("a project board", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  await page.goto("/tasks");
  const board = page.locator("ortus-tasks-board");
  // No project yet: the empty state offers one.
  await board.getByRole("button", { name: en.newProject }).click();
  const made = board.getByRole("dialog", { name: en.newProject });
  await made.getByLabel(en.projectName).fill("Flood response 2026");
  await made.getByRole("button", { name: en.create }).click();
  await expect(board.getByRole("combobox", { name: en.project, exact: true })).toHaveValue(/.+/);

  const add = async (title: string, opts: { who?: string; due?: string; column?: string; checklist?: string } = {}) => {
    await board.getByRole("button", { name: en.newTask }).click();
    const d = board.getByRole("dialog", { name: en.newTask });
    await d.getByLabel(en.title).fill(title);
    if (opts.who) await d.getByLabel(en.assignee).fill(opts.who);
    if (opts.due) await d.getByLabel(en.dueAt).fill(opts.due);
    if (opts.column) await d.getByLabel(en.column).selectOption(opts.column);
    if (opts.checklist) await d.getByLabel(en.checklistLines).fill(opts.checklist);
    await d.getByRole("button", { name: en.save }).click();
    await expect(d).toBeHidden();
  };
  const today = new Date();
  const at = (days: number, hour: number) => {
    const d = new Date(today.getFullYear(), today.getMonth(), today.getDate() + days, hour, 0);
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}T${String(hour).padStart(2, "0")}:00`;
  };
  await add("Book 6 buses for evacuation", { who: "olena@ministry.example", due: at(3, 12) });
  await add("Close road R-12 and divert traffic", { who: "olena@ministry.example", due: at(0, 17), checklist: "Barriers at both ends\nSigns for the detour\nTell the police\nRadio announcement" });
  await add("Hydrology: confirm 3.8 m threshold", { due: at(-1, 9), column: "waiting" });
  await add("Shelter capacity sheet", { column: "done" });

  const column = (name: string) => board.getByRole("region", { name });
  await expect(column(en.todo).getByRole("button")).toHaveCount(2);
  await expect(column(en.waiting)).toContainText("Overdue 1 day");
  await expect(column(en.todo)).toContainText("0 of 4");

  // Order in a column without dragging: up in the task's dialog.
  await expect(column(en.todo).getByRole("button").first()).toContainText("Book 6 buses");
  await column(en.todo).getByRole("button", { name: /Close road R-12/ }).click();
  const moving = board.getByRole("dialog", { name: "Close road R-12 and divert traffic" });
  await expect(moving.getByRole("button", { name: en.moveDown })).toBeDisabled();
  await moving.getByRole("button", { name: en.moveUp }).click();
  await expect(column(en.todo).getByRole("button").first()).toContainText("Close road R-12");
  await expect(moving.getByRole("button", { name: en.moveUp })).toBeDisabled();
  await expect(moving.getByRole("button", { name: en.moveDown })).toBeFocused();
  await moving.getByRole("button", { name: en.cancel }).click();

  // Dragged to In progress.
  await column(en.todo).getByRole("button", { name: /Close road R-12/ }).dragTo(column(en.doing));
  await expect(column(en.doing)).toContainText("Close road R-12");
  await expect(column(en.doing)).toContainText(en.today);

  // The checklist, ticked in the task.
  await column(en.doing).getByRole("button", { name: /Close road R-12/ }).click();
  const task = board.getByRole("dialog", { name: "Close road R-12 and divert traffic" });
  await task.getByRole("checkbox", { name: "Barriers at both ends" }).check();
  await task.getByRole("checkbox", { name: "Signs for the detour" }).check();
  await task.getByRole("button", { name: en.cancel }).click();
  await expect(column(en.doing)).toContainText("2 of 4");
  await page.screenshot({ path: "test-results/board.png" });

  // The list, the timeline, one's own tasks.
  await board.getByRole("tab", { name: en.list }).click();
  await expect(board.getByRole("table")).toContainText("Book 6 buses for evacuation");
  await board.getByRole("tab", { name: en.timeline }).click();
  await expect(board.getByRole("table", { name: en.timeline })).toContainText("Hydrology");
  await board.getByRole("tab", { name: /My tasks/ }).click();
  await expect(board.getByRole("tab", { name: /My tasks · 2/ })).toBeVisible();
  await expect(board.getByRole("table")).toContainText("Close road R-12 and divert traffic");
  await expect(board.getByRole("table")).not.toContainText("Hydrology");
  expect(errors).toEqual([]);
});

test("the board on a phone: columns side by side in their own scroll, the task dialog fits", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.setViewportSize({ width: 390, height: 844 });
  const fits = async () => expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
  await page.goto("/tasks");
  const board = page.locator("ortus-tasks-board");
  const column = (name: string) => board.getByRole("region", { name });
  await expect(column(en.todo)).toBeVisible();
  await fits();
  // The columns scroll inside the board, one at a time.
  const columns = board.locator(".columns");
  expect(await columns.evaluate((e) => e.scrollWidth > e.clientWidth)).toBe(true);
  await page.screenshot({ path: "test-results/phone-board.png" });
  await column(en.doing).scrollIntoViewIfNeeded();
  await expect(column(en.doing)).toBeInViewport();
  await page.screenshot({ path: "test-results/phone-board-doing.png" });
  // A task: its dialog fits the screen.
  await column(en.doing).getByRole("button", { name: /Close road R-12/ }).click();
  const task = board.getByRole("dialog", { name: "Close road R-12 and divert traffic" });
  await expect(task).toBeVisible();
  const box = (await task.boundingBox())!;
  expect(box.x).toBeGreaterThanOrEqual(0);
  expect(box.x + box.width).toBeLessThanOrEqual(390);
  await fits();
  await page.screenshot({ path: "test-results/phone-task.png" });
  await task.getByRole("button", { name: en.cancel }).click();
  await expect(task).toBeHidden();
  // The list and the timeline scroll in their own box.
  await board.getByRole("tab", { name: en.list }).click();
  await expect(board.getByRole("table")).toContainText("Book 6 buses for evacuation");
  await fits();
  await page.screenshot({ path: "test-results/phone-list.png" });
  await board.getByRole("tab", { name: en.timeline }).click();
  await expect(board.getByRole("table", { name: en.timeline })).toContainText("Hydrology");
  await fits();
  await page.screenshot({ path: "test-results/phone-timeline.png" });
  expect(errors).toEqual([]);
});
