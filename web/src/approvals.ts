// Approvals: what waits for one's decision or signature (approval routes
// of documents), and one route opened here, shown by the docs app's own
// review element (board WebApprovals).
import { LitElement, LocalizeController, call, controls, css, customElement, html, nothing, state } from "ortus-kit";
import { t } from "./i18n.js";

interface Waiting {
  id: string;
  name: string;
  by: string;
  at: number;
  steps: { state: string; user: string; kind: string; title: string; dueAt: number | null }[];
}

@customElement("ortus-approvals-home")
export class OrtusApprovalsHome extends LitElement {
  static override styles = [
    controls,
    css`
      :host {
        display: block;
        padding: 32px;
        color: var(--ortus-text);
      }
      h1 {
        margin-block: 0 24px;
        font-size: 24px;
      }
      ul {
        display: grid;
        gap: 10px;
        max-inline-size: 760px;
        margin: 0;
        padding: 0;
        list-style: none;
      }
      a.item {
        display: grid;
        gap: 4px;
        padding: 18px;
        border-radius: 14px;
        border: 1px solid var(--ortus-line);
        background: var(--ortus-surface);
        color: inherit;
        text-decoration: none;
      }
      small,
      p {
        color: var(--ortus-text-3);
      }
      .kind {
        color: var(--ortus-accent);
        font-weight: 600;
      }
    `,
  ];

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
    return html`<h1>${t(this.localize, "approvals")}</h1>
      ${this.waiting.length
        ? html`<ul aria-label=${t(this.localize, "waitingForYou")}>
            ${this.waiting.map((w) => {
              const mine = w.steps.find((s) => s.state === "current");
              return html`<li>
                <a class="item" href=${`/approvals?review=${encodeURIComponent(w.id)}`}>
                  <strong>${w.name}</strong>
                  <small><span class="kind">${mine?.kind === "sign" ? t(this.localize, "toSign") : t(this.localize, "toApprove")}</span> · ${t(this.localize, "sentByName", { who: w.by })}</small>
                </a>
              </li>`;
            })}
          </ul>`
        : this.loaded
          ? html`<p role="status">${t(this.localize, "nothingWaiting")}</p>`
          : nothing}`;
  }
}
