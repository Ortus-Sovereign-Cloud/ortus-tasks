// Tasks (board WebTasks): a project's board in four columns, its list,
// a timeline by due time, and one's own tasks; a task opens in a dialog
// with its notes, who does it, when it is due and its checklist.
import { ApiError, LitElement, LocalizeController, call, controls, css, customElement, html, nothing, state, type TemplateResult } from "ortus-kit";
import { t } from "./i18n.js";
import type { TasksKey } from "./locales/en.js";

type Column = "todo" | "doing" | "waiting" | "done";
type View = "board" | "list" | "timeline" | "mine";

interface Task {
  id: string;
  project: string;
  projectName?: string;
  title: string;
  notes: string;
  assignee: string | null;
  dueAt: number | null;
  state: Column;
  checked?: [number, number];
  checks?: { n: number; text: string; done: boolean }[];
  people?: Record<string, { name: string; email: string }>;
  source?: string | null;
}

interface Project {
  id: string;
  name: string;
  owner: string;
  members: string[];
  tasks: Task[];
  people: Record<string, { name: string; email: string }>;
}

const COLUMNS: [Column, TasksKey, string][] = [
  ["todo", "todo", "var(--ortus-text-3)"],
  ["doing", "doing", "var(--ortus-accent)"],
  ["waiting", "waiting", "var(--ortus-orange)"],
  ["done", "done", "var(--ortus-teal)"],
];

const TONES = ["#6ee0d0", "#a5b4ff", "#f9b77a", "#d4f27a", "#ffa39a", "#eceef3"];

function tone(text: string): string {
  let h = 0;
  for (const c of text) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return TONES[h % TONES.length]!;
}

function initials(name: string): string {
  const w = name.split(/[\s@.]+/).filter(Boolean);
  return ((w[0]?.[0] ?? "?") + (w.length > 1 ? (w[1]?.[0] ?? "") : "")).toUpperCase();
}

function lang(): string {
  return document.documentElement.lang || "en";
}

function dayStart(ms: number): number {
  const d = new Date(ms);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

/** "Today", "Fri", "Overdue 1 day", a date further off; and how urgent it is. */
function dueText(localize: LocalizeController, due: number, done: boolean): { text: string; kind: "" | "soon" | "late" } {
  const days = Math.round((dayStart(due) - dayStart(Date.now())) / 86_400_000);
  if (!done && days < 0) return { text: t(localize, "overdue", { count: -days }), kind: "late" };
  if (days === 0) return { text: t(localize, "today"), kind: done ? "" : "soon" };
  if (days === 1) return { text: t(localize, "tomorrow"), kind: "" };
  if (days > 1 && days < 7) return { text: new Intl.DateTimeFormat(lang(), { weekday: "short" }).format(new Date(due)), kind: "" };
  return { text: new Intl.DateTimeFormat(lang(), { day: "numeric", month: "short" }).format(new Date(due)), kind: "" };
}

function toLocalInput(ms: number | null): string {
  if (!ms) return "";
  const d = new Date(ms - new Date(ms).getTimezoneOffset() * 60_000);
  return d.toISOString().slice(0, 16);
}

@customElement("ortus-tasks-board")
export class OrtusTasksBoard extends LitElement {
  static override styles = [
    controls,
    css`
      :host {
        display: block;
        min-block-size: var(--ortus-screen, 100vh);
        color: var(--ortus-text);
      }
      header {
        display: flex;
        align-items: center;
        gap: 16px;
        padding-block: 14px;
        padding-inline: 24px;
        border-block-end: 1px solid var(--ortus-line);
      }
      .crumbs {
        display: flex;
        align-items: center;
        gap: 12px;
        font-size: 15px;
        font-weight: 600;
      }
      .crumbs span {
        color: var(--ortus-text-3);
        font-weight: 400;
      }
      .crumbs select {
        border: 0;
        background: transparent;
        color: var(--ortus-text);
        font: inherit;
        font-weight: 600;
        cursor: pointer;
      }
      .tabs {
        display: flex;
        gap: 4px;
        padding: 4px;
        border-radius: 12px;
        border: 1px solid var(--ortus-line);
        background: var(--ortus-surface);
      }
      .tabs button {
        padding-block: 6px;
        padding-inline: 14px;
        border: 0;
        border-radius: 8px;
        background: none;
        color: var(--ortus-text-3);
        font: inherit;
        font-size: 14px;
        font-weight: 600;
        cursor: pointer;
      }
      .tabs button[aria-selected="true"] {
        background: var(--ortus-surface-2);
        color: var(--ortus-text);
      }
      .spacer {
        flex: 1;
      }
      .columns {
        display: grid;
        grid-template-columns: repeat(4, minmax(0, 1fr));
        gap: 16px;
        padding: 24px;
        align-items: start;
      }
      .column {
        display: grid;
        gap: 12px;
        padding: 16px;
        border-radius: 18px;
        border: 1px solid var(--ortus-line);
        background: var(--ortus-surface);
      }
      .column.over {
        border-color: var(--ortus-accent);
      }
      .column h2 {
        display: flex;
        align-items: center;
        gap: 8px;
        margin: 0;
        padding-inline: 4px;
        font-size: 15px;
      }
      .column h2 i {
        inline-size: 8px;
        block-size: 8px;
        border-radius: 50%;
      }
      .column h2 small {
        margin-inline-start: auto;
        color: var(--ortus-text-3);
        font-weight: 400;
      }
      .card {
        display: grid;
        gap: 12px;
        padding: 16px;
        border-radius: 14px;
        border: 1px solid var(--ortus-line);
        background: var(--ortus-surface-2);
        color: inherit;
        font: inherit;
        text-align: start;
        cursor: pointer;
      }
      .card:focus-visible {
        outline: 2px solid var(--ortus-accent);
      }
      .card strong {
        font-size: 15px;
        font-weight: 600;
      }
      .meta {
        display: flex;
        align-items: center;
        gap: 10px;
        font-size: 13px;
        color: var(--ortus-text-3);
      }
      .avatar {
        display: grid;
        place-items: center;
        inline-size: 28px;
        block-size: 28px;
        border-radius: 8px;
        color: #0a0b10;
        font-size: 11px;
        font-weight: 700;
      }
      .soon {
        color: var(--ortus-orange);
      }
      .late {
        color: var(--ortus-red);
      }
      .count {
        margin-inline-start: auto;
      }
      table {
        inline-size: calc(100% - 48px);
        margin: 24px;
        border-collapse: collapse;
        font-size: 14px;
      }
      th,
      td {
        padding-block: 12px;
        padding-inline: 12px;
        border-block-end: 1px solid var(--ortus-line);
        text-align: start;
      }
      th {
        color: var(--ortus-text-3);
        font-weight: 500;
      }
      td button.link {
        border: 0;
        background: none;
        color: var(--ortus-text);
        font: inherit;
        font-weight: 600;
        cursor: pointer;
        padding: 0;
        text-align: start;
      }
      .timeline {
        display: grid;
        gap: 8px;
        padding: 24px;
      }
      .days,
      .lane {
        display: grid;
        grid-template-columns: 260px repeat(14, minmax(0, 1fr));
        align-items: center;
        gap: 2px;
        font-size: 13px;
      }
      .days span {
        color: var(--ortus-text-3);
        text-align: center;
      }
      .days span.now {
        color: var(--ortus-accent);
        font-weight: 600;
      }
      .lane > button {
        justify-self: start;
      }
      .bar {
        block-size: 26px;
        border-radius: 8px;
        background: color-mix(in srgb, var(--ortus-accent) 70%, transparent);
      }
      .bar.late {
        background: color-mix(in srgb, var(--ortus-red) 70%, transparent);
      }
      .bar.done {
        background: color-mix(in srgb, var(--ortus-teal) 60%, transparent);
      }
      .empty {
        display: grid;
        gap: 12px;
        justify-items: start;
        max-inline-size: 520px;
        margin: 64px;
      }
      .empty h1 {
        margin: 0;
        font-size: 24px;
      }
      .empty p {
        margin: 0;
        color: var(--ortus-text-3);
      }
      dialog {
        inline-size: min(560px, 94vw);
        padding: 0;
        border-radius: 22px;
        border: 1px solid var(--ortus-line-strong);
        background: var(--ortus-surface);
        color: var(--ortus-text);
      }
      dialog::backdrop {
        background: rgb(0 0 0 / 0.55);
      }
      dialog form {
        display: grid;
        gap: 14px;
        padding: 26px;
      }
      dialog h2 {
        margin: 0;
        font-size: 20px;
      }
      label.field {
        display: grid;
        gap: 6px;
        font-size: 13px;
        color: var(--ortus-text-2);
      }
      .pair {
        display: grid;
        grid-template-columns: 1fr 1fr;
        gap: 12px;
      }
      textarea {
        font: inherit;
      }
      .checks {
        display: grid;
        gap: 8px;
        margin: 0;
        padding: 0;
        list-style: none;
      }
      .checks label {
        display: flex;
        gap: 10px;
        align-items: center;
        font-size: 14px;
      }
      .checks input[type="checkbox"] {
        flex: none;
        inline-size: 18px;
        block-size: 18px;
        margin: 0;
        padding: 0;
        accent-color: var(--ortus-accent);
      }
      .checks .done {
        text-decoration: line-through;
        color: var(--ortus-text-3);
      }
      .row {
        display: flex;
        gap: 10px;
      }
      .row .grow {
        flex: 1;
      }
      .error {
        margin: 0;
        color: var(--ortus-red);
        font-size: 14px;
      }
      .from {
        font-size: 13px;
      }
      .from a {
        color: var(--ortus-accent);
      }
      .scroll {
        overflow-x: auto;
      }
      /* A phone-sized screen: the toolbar wraps, the board's columns scroll
         sideways one at a time, tables and the timeline scroll in their box. */
      @media (max-width: 720px) {
        header {
          flex-wrap: wrap;
          gap: 10px;
          padding-block: 10px;
          padding-inline: 12px;
        }
        .spacer {
          display: none;
        }
        .tabs {
          max-inline-size: 100%;
          overflow-x: auto;
        }
        .tabs button {
          flex: none;
          min-block-size: 44px;
        }
        .button,
        .crumbs select {
          block-size: 44px;
        }
        .columns {
          grid-template-columns: none;
          grid-auto-flow: column;
          grid-auto-columns: calc(100% - 40px);
          gap: 12px;
          padding: 12px;
          overflow-x: auto;
          scroll-snap-type: inline mandatory;
          scroll-padding-inline: 12px;
        }
        .column {
          scroll-snap-align: start;
        }
        table {
          inline-size: max-content;
          min-inline-size: calc(100% - 24px);
          margin: 12px;
        }
        td button.link {
          min-block-size: 44px;
        }
        .timeline {
          padding: 12px;
          overflow-x: auto;
        }
        .days,
        .lane {
          grid-template-columns: 150px repeat(14, 28px);
        }
        .lane > button {
          min-block-size: 44px;
          overflow-wrap: anywhere;
        }
        .empty {
          margin: 24px;
        }
        dialog {
          max-inline-size: calc(100vw - 16px);
        }
        dialog form {
          padding: 18px;
        }
        dialog select {
          block-size: 44px;
        }
        .pair {
          grid-template-columns: minmax(0, 1fr);
        }
      }
    `,
  ];

  private readonly localize = new LocalizeController(this);
  @state() private projects: { id: string; name: string }[] = [];
  @state() private project: Project | null = null;
  @state() private mine: Task[] = [];
  @state() private openCount = 0;
  @state() private view: View = "board";
  @state() private editing: Task | null = null;
  @state() private creating = false;
  @state() private over: Column | null = null;
  @state() private error = "";
  @state() private loaded = false;
  private dragged: string | null = null;

  override connectedCallback(): void {
    super.connectedCallback();
    void this.load();
  }

  private params(): URLSearchParams {
    return new URLSearchParams(location.search);
  }

  private async load(): Promise<void> {
    try {
      const r = await call<{ projects: { id: string; name: string }[]; mine: number }>("GET", "/tasks/projects");
      this.projects = r.projects;
      this.openCount = r.mine;
      const p = this.params();
      if (p.get("view") === "mine") this.view = "mine";
      const task = p.get("task");
      if (task) {
        const one = await call<Task>("GET", `/tasks/tasks/${encodeURIComponent(task)}`);
        await this.openProject(one.project);
        this.editing = one;
      } else {
        const id = p.get("project") ?? r.projects[0]?.id;
        if (id) await this.openProject(id);
      }
      if (this.view === "mine") await this.loadMine();
    } catch (e) {
      this.error = e instanceof ApiError ? e.message : String(e);
    } finally {
      this.loaded = true;
    }
  }

  private async openProject(id: string): Promise<void> {
    this.project = await call<Project>("GET", `/tasks/projects/${encodeURIComponent(id)}`);
    const url = new URL(location.href);
    url.searchParams.set("project", id);
    url.searchParams.delete("task");
    history.replaceState(null, "", url);
  }

  private async loadMine(): Promise<void> {
    this.mine = (await call<{ tasks: Task[] }>("GET", "/tasks/mine")).tasks;
  }

  private async refresh(): Promise<void> {
    if (this.project) await this.openProject(this.project.id);
    const r = await call<{ projects: { id: string; name: string }[]; mine: number }>("GET", "/tasks/projects");
    this.projects = r.projects;
    this.openCount = r.mine;
    if (this.view === "mine") await this.loadMine();
  }

  private person(id: string | null): { name: string; email: string } | null {
    if (!id) return null;
    return this.project?.people[id] ?? this.editing?.people?.[id] ?? null;
  }

  private async open(task: Task): Promise<void> {
    this.error = "";
    this.editing = await call<Task>("GET", `/tasks/tasks/${encodeURIComponent(task.id)}`);
  }

  private async move(id: string, to: Column, before?: string): Promise<void> {
    try {
      await call("PATCH", `/tasks/tasks/${encodeURIComponent(id)}`, { state: to, before });
      await this.refresh();
    } catch (e) {
      this.error = e instanceof ApiError ? e.message : String(e);
    }
  }

  private async createProject(e: Event): Promise<void> {
    e.preventDefault();
    const form = e.target as HTMLFormElement;
    const name = (form.elements.namedItem("name") as HTMLInputElement).value;
    const members = (form.elements.namedItem("members") as HTMLInputElement).value.split(/[\s,;]+/).filter(Boolean);
    try {
      const made = await call<{ id: string }>("POST", "/tasks/projects", { name, members });
      this.renderRoot.querySelector<HTMLDialogElement>("dialog.project")?.close();
      form.reset();
      await this.openProject(made.id);
      await this.refresh();
      this.view = "board";
    } catch (err) {
      this.error = err instanceof ApiError ? err.message : String(err);
    }
  }

  private fields(form: HTMLFormElement): Record<string, unknown> {
    const v = (n: string) => (form.elements.namedItem(n) as HTMLInputElement | null)?.value ?? "";
    const due = v("due");
    return { title: v("title"), notes: v("notes"), assignee: v("assignee"), dueAt: due ? new Date(due).getTime() : null, state: v("state") };
  }

  private async save(e: Event): Promise<void> {
    e.preventDefault();
    const form = e.target as HTMLFormElement;
    const f = this.fields(form);
    try {
      if (this.creating) {
        const checklist = ((form.elements.namedItem("checklist") as HTMLTextAreaElement | null)?.value ?? "").split("\n").map((x) => x.trim()).filter(Boolean);
        await call("POST", "/tasks/tasks", { ...f, project: this.project?.id, assignee: f.assignee || undefined, dueAt: f.dueAt ?? undefined, checklist });
      } else if (this.editing) {
        const changes: Record<string, unknown> = { title: f.title, notes: f.notes, assignee: f.assignee, dueAt: f.dueAt };
        if (f.state !== this.editing.state) changes.state = f.state;
        await call("PATCH", `/tasks/tasks/${encodeURIComponent(this.editing.id)}`, changes);
      }
      this.close();
      await this.refresh();
    } catch (err) {
      this.error = err instanceof ApiError ? err.message : String(err);
    }
  }

  private async tick(n: number, done: boolean): Promise<void> {
    if (!this.editing) return;
    this.editing = await call<Task>("PATCH", `/tasks/tasks/${encodeURIComponent(this.editing.id)}/checks/${n}`, { done });
    await this.refresh();
  }

  private async addCheck(e: Event): Promise<void> {
    e.preventDefault();
    const input = this.renderRoot.querySelector<HTMLInputElement>("input.new-check");
    if (!this.editing || !input?.value.trim()) return;
    this.editing = await call<Task>("POST", `/tasks/tasks/${encodeURIComponent(this.editing.id)}/checks`, { text: input.value });
    input.value = "";
    await this.refresh();
  }

  private async deleteTask(): Promise<void> {
    if (!this.editing) return;
    await call("DELETE", `/tasks/tasks/${encodeURIComponent(this.editing.id)}`);
    this.close();
    await this.refresh();
  }

  private close(): void {
    this.editing = null;
    this.creating = false;
    this.error = "";
  }

  override updated(): void {
    const dialog = this.renderRoot.querySelector<HTMLDialogElement>("dialog.task");
    if (dialog && (this.editing || this.creating) && !dialog.open) dialog.showModal();
  }

  private avatar(id: string | null): TemplateResult | typeof nothing {
    const p = this.person(id);
    if (!p) return nothing;
    return html`<span class="avatar" title=${p.name} .style=${`background: ${tone(p.name)}`}>${initials(p.name)}</span>`;
  }

  private card(task: Task): TemplateResult {
    const due = task.dueAt ? dueText(this.localize, task.dueAt, task.state === "done") : null;
    const [done, all] = task.checked ?? [0, 0];
    return html`<button
      class="card"
      draggable="true"
      @dragstart=${(e: DragEvent) => {
        this.dragged = task.id;
        e.dataTransfer?.setData("text/plain", task.id);
      }}
      @dragover=${(e: DragEvent) => e.preventDefault()}
      @drop=${(e: DragEvent) => {
        e.preventDefault();
        e.stopPropagation();
        this.over = null;
        const id = this.dragged;
        this.dragged = null;
        if (id && id !== task.id) void this.move(id, task.state, task.id);
      }}
      @click=${() => this.open(task)}
    >
      <strong>${task.title}</strong>
      <span class="meta">
        ${this.avatar(task.assignee)} ${due ? html`<span class=${due.kind}>${due.text}</span>` : nothing}
        ${all ? html`<span class="count">${t(this.localize, "checkedOf", { done, all })}</span>` : nothing}
      </span>
    </button>`;
  }

  /** Up and down in its column without dragging (WCAG 2.5.7). */
  private order(x: Task): TemplateResult | typeof nothing {
    const cards = (this.project?.tasks ?? []).filter((c) => c.state === x.state);
    const at = cards.findIndex((c) => c.id === x.id);
    if (at < 0 || cards.length < 2) return nothing;
    const go = async (before: string | undefined): Promise<void> => {
      await this.move(x.id, x.state, before);
      await this.updateComplete;
      // At the top or the bottom its button turns off: focus stays in the group.
      const buttons = [...this.renderRoot.querySelectorAll<HTMLButtonElement>(".order button")];
      const focused = buttons.find((b) => b.matches(":focus"));
      if (!focused || focused.disabled) buttons.find((b) => !b.disabled)?.focus();
    };
    return html`<div class="row order" role="group" aria-label=${t(this.localize, "orderInColumn")}>
      <button type="button" class="button" ?disabled=${at === 0} @click=${() => go(cards[at - 1]!.id)}>${t(this.localize, "moveUp")}</button>
      <button type="button" class="button" ?disabled=${at === cards.length - 1} @click=${() => go(cards[at + 2]?.id)}>${t(this.localize, "moveDown")}</button>
    </div>`;
  }

  private board(p: Project): TemplateResult {
    return html`<div class="columns">
      ${COLUMNS.map(([col, key, color]) => {
        const cards = p.tasks.filter((x) => x.state === col);
        return html`<section
          class="column ${this.over === col ? "over" : ""}"
          aria-label=${t(this.localize, key)}
          @dragover=${(e: DragEvent) => {
            e.preventDefault();
            this.over = col;
          }}
          @dragleave=${() => (this.over = null)}
          @drop=${(e: DragEvent) => {
            e.preventDefault();
            this.over = null;
            const id = this.dragged;
            this.dragged = null;
            if (id) void this.move(id, col);
          }}
        >
          <h2><i .style=${`background: ${color}`}></i>${t(this.localize, key)}<small>${cards.length}</small></h2>
          ${cards.map((c) => this.card(c))}
        </section>`;
      })}
    </div>`;
  }

  private list(tasks: Task[], withProject = false): TemplateResult {
    return html`<div class="scroll"><table>
      <thead>
        <tr>
          <th>${t(this.localize, "task")}</th>
          ${withProject ? html`<th>${t(this.localize, "project")}</th>` : nothing}
          <th>${t(this.localize, "who")}</th>
          <th>${t(this.localize, "due")}</th>
          <th>${t(this.localize, "column")}</th>
        </tr>
      </thead>
      <tbody>
        ${tasks.map((x) => {
          const due = x.dueAt ? dueText(this.localize, x.dueAt, x.state === "done") : null;
          const col = COLUMNS.find(([c]) => c === x.state)!;
          return html`<tr>
            <td><button class="link" @click=${async () => (withProject ? (await this.openProject(x.project), this.open(x)) : this.open(x))}>${x.title}</button></td>
            ${withProject ? html`<td>${x.projectName ?? ""}</td>` : nothing}
            <td>${this.person(x.assignee)?.name ?? ""}</td>
            <td class=${due?.kind ?? ""}>${due?.text ?? ""}</td>
            <td>${t(this.localize, col[1])}</td>
          </tr>`;
        })}
      </tbody>
    </table></div>`;
  }

  private timeline(p: Project): TemplateResult {
    const start = dayStart(Date.now()) - 2 * 86_400_000;
    const days = Array.from({ length: 14 }, (_, i) => start + i * 86_400_000);
    const fmt = new Intl.DateTimeFormat(lang(), { weekday: "narrow", day: "numeric" });
    const dated = p.tasks.filter((x) => x.dueAt).sort((a, b) => a.dueAt! - b.dueAt!);
    return html`<div class="timeline" role="table" aria-label=${t(this.localize, "timeline")}>
      <div class="days" role="row"><span></span>${days.map((d) => html`<span class=${d === dayStart(Date.now()) ? "now" : ""}>${fmt.format(new Date(d))}</span>`)}</div>
      ${dated.map((x) => {
        const created = Math.max(start, dayStart((x as Task & { createdAt?: number }).createdAt ?? Date.now()));
        const from = Math.max(0, Math.round((created - start) / 86_400_000));
        const to = Math.min(13, Math.max(from, Math.round((dayStart(x.dueAt!) - start) / 86_400_000)));
        const late = x.state !== "done" && x.dueAt! < Date.now();
        return html`<div class="lane" role="row">
          <button class="link" @click=${() => this.open(x)}>${x.title}</button>
          <span class="bar ${x.state === "done" ? "done" : late ? "late" : ""}" .style=${`grid-column: ${from + 2} / ${to + 3}`} title=${x.title}></span>
        </div>`;
      })}
      ${dated.length ? nothing : html`<p>${t(this.localize, "noDueTimes")}</p>`}
    </div>`;
  }

  private dialog(): TemplateResult | typeof nothing {
    const x = this.editing;
    if (!x && !this.creating) return nothing;
    const who = x?.assignee ? (this.person(x.assignee)?.email ?? "") : "";
    return html`<dialog class="task" aria-label=${x ? x.title : t(this.localize, "newTask")} @close=${() => this.close()}>
      <form @submit=${this.save}>
        <h2>${x ? t(this.localize, "editTask") : t(this.localize, "newTask")}</h2>
        <label class="field">${t(this.localize, "title")}<input class="input" name="title" required .value=${x?.title ?? ""} /></label>
        <label class="field">${t(this.localize, "notes")}<textarea class="input" name="notes" rows="3" .value=${x?.notes ?? ""}></textarea></label>
        <div class="pair">
          <label class="field">${t(this.localize, "assignee")}<input class="input" name="assignee" type="email" .value=${who} placeholder=${t(this.localize, "address")} /></label>
          <label class="field">${t(this.localize, "dueAt")}<input class="input" name="due" type="datetime-local" .value=${toLocalInput(x?.dueAt ?? null)} /></label>
        </div>
        <label class="field">${t(this.localize, "column")}
          <select class="input" name="state" .value=${x?.state ?? "todo"}>
            ${COLUMNS.map(([c, key]) => html`<option value=${c} ?selected=${c === (x?.state ?? "todo")}>${t(this.localize, key)}</option>`)}
          </select>
        </label>
        ${x ? this.order(x) : nothing}
        ${x
          ? html`<fieldset class="field">
              <legend>${t(this.localize, "checklist")}</legend>
              <ul class="checks">
                ${(x.checks ?? []).map((c) => html`<li><label class=${c.done ? "done" : ""}><input type="checkbox" .checked=${c.done} @change=${(e: Event) => this.tick(c.n, (e.target as HTMLInputElement).checked)} />${c.text}</label></li>`)}
              </ul>
              <div class="row"><input class="input new-check grow" aria-label=${t(this.localize, "newItem")} placeholder=${t(this.localize, "newItem")} @keydown=${(e: KeyboardEvent) => e.key === "Enter" && this.addCheck(e)} /><button type="button" class="button" @click=${this.addCheck}>${t(this.localize, "add")}</button></div>
            </fieldset>`
          : html`<label class="field">${t(this.localize, "checklistLines")}<textarea class="input" name="checklist" rows="3"></textarea></label>`}
        ${x?.source ? html`<p class="from"><a href=${x.source}>${t(this.localize, "whereFrom")}</a></p>` : nothing}
        ${this.error ? html`<p class="error" role="alert">${this.error}</p>` : nothing}
        <div class="row">
          ${x ? html`<button type="button" class="button" @click=${this.deleteTask}>${t(this.localize, "delete")}</button>` : nothing}
          <span class="grow"></span>
          <button type="button" class="button" @click=${() => this.renderRoot.querySelector<HTMLDialogElement>("dialog.task")?.close()}>${t(this.localize, "cancel")}</button>
          <button class="button primary">${t(this.localize, "save")}</button>
        </div>
      </form>
    </dialog>`;
  }

  override render() {
    const p = this.project;
    const tabs: [View, string][] = [
      ["board", t(this.localize, "board")],
      ["list", t(this.localize, "list")],
      ["timeline", t(this.localize, "timeline")],
      ["mine", t(this.localize, "myTasks", { count: this.openCount })],
    ];
    const projectDialog = html`<dialog class="project" aria-label=${t(this.localize, "newProject")}>
      <form @submit=${this.createProject}>
        <h2>${t(this.localize, "newProject")}</h2>
        <label class="field">${t(this.localize, "projectName")}<input class="input" name="name" required /></label>
        <label class="field">${t(this.localize, "members")}<input class="input" name="members" placeholder=${t(this.localize, "addresses")} /></label>
        ${this.error ? html`<p class="error" role="alert">${this.error}</p>` : nothing}
        <div class="row"><span class="grow"></span><button type="button" class="button" @click=${() => this.renderRoot.querySelector<HTMLDialogElement>("dialog.project")?.close()}>${t(this.localize, "cancel")}</button><button class="button primary">${t(this.localize, "create")}</button></div>
      </form>
    </dialog>`;
    if (this.loaded && !p && this.view !== "mine") {
      return html`<div class="empty">
          <h1>${t(this.localize, "tasks")}</h1>
          <p>${t(this.localize, "emptyLead")}</p>
          <button class="button primary" @click=${() => this.renderRoot.querySelector<HTMLDialogElement>("dialog.project")?.showModal()}>${t(this.localize, "newProject")}</button>
        </div>
        ${projectDialog}`;
    }
    return html`<header>
        <nav class="crumbs" aria-label=${t(this.localize, "where")}>
          ${t(this.localize, "tasks")} <span>/</span>
          <select aria-label=${t(this.localize, "project")} @change=${async (e: Event) => {
            const v = (e.target as HTMLSelectElement).value;
            if (v === "") this.renderRoot.querySelector<HTMLDialogElement>("dialog.project")?.showModal();
            else await this.openProject(v);
          }}>
            ${this.projects.map((x) => html`<option value=${x.id} ?selected=${x.id === p?.id}>${x.name}</option>`)}
            <option value="">${t(this.localize, "newProjectOption")}</option>
          </select>
        </nav>
        <div class="tabs" role="tablist">
          ${tabs.map(([v, label]) => html`<button role="tab" aria-selected=${this.view === v ? "true" : "false"} @click=${async () => ((this.view = v), v === "mine" ? await this.loadMine() : undefined)}>${label}</button>`)}
        </div>
        <span class="spacer"></span>
        ${p ? html`<button class="button primary" @click=${() => ((this.creating = true), (this.editing = null))}>${t(this.localize, "newTask")}</button>` : nothing}
      </header>
      ${this.error && !this.editing && !this.creating ? html`<p class="error" role="alert">${this.error}</p>` : nothing}
      ${this.view === "mine" ? this.list(this.mine, true) : p ? (this.view === "board" ? this.board(p) : this.view === "list" ? this.list(p.tasks) : this.timeline(p)) : nothing}
      ${this.dialog()} ${projectDialog}`;
  }
}
