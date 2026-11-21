// The approvals card on the start page (ortus-approvals-today, from
// /assets/apps/approvals/today.js): what waits for one's decision or
// signature.
import { LitElement, LocalizeController, call, customElement, html, nothing, state } from "ortus-kit";
import { ready, t } from "./i18n.js";
import { cardStyles } from "./today-card.js";

await ready;

interface Waiting {
  id: string;
  name: string;
  by: string;
  steps: { state: string; kind: string }[];
}

@customElement("ortus-approvals-today")
export class OrtusApprovalsToday extends LitElement {
  static override styles = cardStyles;

  private readonly localize = new LocalizeController(this);
  @state() private waiting: Waiting[] = [];
  @state() private loaded = false;

  override connectedCallback(): void {
    super.connectedCallback();
    void call<{ waiting: Waiting[] }>("GET", "/docs/reviews/mine")
      .then((r) => (this.waiting = r.waiting))
      .catch(() => undefined)
      .finally(() => (this.loaded = true));
  }

  override render() {
    if (!this.loaded) return nothing;
    return html`
      <section aria-labelledby="approvals-today">
        <header>
          <h2 id="approvals-today">${t(this.localize, "waitingForYou")}</h2>
          ${this.waiting.length ? html`<span class="count">${this.waiting.length}</span>` : nothing}
        </header>
        ${this.waiting.length
          ? html`<ul>
              ${this.waiting.slice(0, 4).map((w) => {
                const mine = w.steps.find((s) => s.state === "current");
                return html`<li>
                  <a href=${`/approvals?review=${encodeURIComponent(w.id)}`}>
                    <strong dir="auto">${w.name}</strong>
                    <small>${mine?.kind === "sign" ? t(this.localize, "toSign") : t(this.localize, "toApprove")} · ${t(this.localize, "sentByName", { who: w.by })}</small>
                  </a>
                </li>`;
              })}
            </ul>`
          : html`<p class="quiet">${t(this.localize, "nothingWaiting")}</p>`}
        <footer><a href="/approvals">${t(this.localize, "todayOpenApprovals")}</a></footer>
      </section>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    "ortus-approvals-today": OrtusApprovalsToday;
  }
}
