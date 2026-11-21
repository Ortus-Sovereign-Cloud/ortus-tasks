// The tasks card on the start page (ortus-tasks-today, from
// /assets/apps/tasks/today.js): one's open tasks, late and due today first.
import { LitElement, LocalizeController, call, customElement, html, nothing, state } from "ortus-kit";
import { ready, t } from "./i18n.js";
import { cardStyles } from "./today-card.js";

await ready;

interface Mine {
  id: string;
  project: string;
  projectName?: string;
  title: string;
  dueAt: number | null;
  state: "todo" | "doing" | "waiting" | "done";
}

@customElement("ortus-tasks-today")
export class OrtusTasksToday extends LitElement {
  static override styles = cardStyles;

  private readonly localize = new LocalizeController(this);
  @state() private open: Mine[] = [];
  @state() private loaded = false;

  override connectedCallback(): void {
    super.connectedCallback();
    void call<{ tasks: Mine[] }>("GET", "/tasks/mine")
      .then((r) => (this.open = r.tasks.filter((x) => x.state !== "done")))
      .catch(() => undefined)
      .finally(() => (this.loaded = true));
  }

  private due(task: Mine): { text: string; late: boolean } | undefined {
    if (!task.dueAt) return undefined;
    const end = new Date();
    end.setHours(23, 59, 59, 999);
    const late = task.dueAt < Date.now();
    const when = new Intl.DateTimeFormat(this.localize.lang, task.dueAt <= end.getTime() && !late ? { hour: "2-digit", minute: "2-digit" } : { day: "numeric", month: "short" }).format(task.dueAt);
    return { text: t(this.localize, late ? "todayLate" : "todayDue", { when }), late };
  }

  override render() {
    if (!this.loaded) return nothing;
    const shown = this.open.slice(0, 5);
    return html`
      <section aria-labelledby="tasks-today">
        <header>
          <h2 id="tasks-today">${t(this.localize, "tasks")}</h2>
          ${this.open.length ? html`<span class="count">${t(this.localize, "todayOpenCount", { count: this.open.length })}</span>` : nothing}
        </header>
        ${shown.length
          ? html`<ul>
              ${shown.map((task) => {
                const due = this.due(task);
                return html`<li>
                  <a href=${`/tasks?project=${encodeURIComponent(task.project)}&task=${encodeURIComponent(task.id)}`}>
                    <strong dir="auto">${task.title}</strong>
                    <small>${task.projectName ?? ""}${due ? html` · <span class=${due.late ? "late" : ""}>${due.text}</span>` : nothing}</small>
                  </a>
                </li>`;
              })}
            </ul>`
          : html`<p class="quiet">${t(this.localize, "todayNoTasks")}</p>`}
        <footer><a href="/tasks?view=mine">${t(this.localize, "todayOpenTasks")}</a></footer>
      </section>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    "ortus-tasks-today": OrtusTasksToday;
  }
}
