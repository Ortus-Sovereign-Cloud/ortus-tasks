// Records: the list of records one may open; a record (board
// WebRecordFiled) with its first page, its facts, what happened since it
// was filed, its signatures checked; and the must-read policies (board
// WebMustRead), read and confirmed one by one.
import { ApiError, LitElement, LocalizeController, call, controls, css, customElement, html, nothing, state } from "ortus-kit";
import { block, type PMNode } from "./doc-view.js";
import { t } from "./i18n.js";
import { drawPdf } from "./pdf.js";

interface RecordItem {
  id: string;
  reference: string;
  title: string;
  kind: string;
  label: string | null;
  filedBy: string;
  filedAt: number;
  keepUntil: number;
  case: string;
  source: string;
  events?: { at: number; kind: string; detail: string }[];
}

interface Check {
  signer: string;
  email: string | null;
  at: number | null;
  reason: string;
  intact: boolean;
  trusted: boolean;
  revoked_at: number | null;
  whole: boolean;
}

interface MustRead {
  id: string;
  title: string;
  note: string;
  body: PMNode;
  minutes: number;
  confirmedAt: number | null;
}

function lang(): string {
  return document.documentElement.lang || "en";
}

function hm(ms: number): string {
  return new Intl.DateTimeFormat(lang(), { hour: "2-digit", minute: "2-digit", hourCycle: "h23" }).format(new Date(ms));
}

function day(ms: number): string {
  return new Intl.DateTimeFormat(lang(), { day: "numeric", month: "short", year: "numeric" }).format(new Date(ms));
}

function stamp(ms: number): string {
  return new Intl.DateTimeFormat(lang(), { day: "numeric", month: "short", year: "numeric", hour: "2-digit", minute: "2-digit", hourCycle: "h23" }).format(new Date(ms));
}

/** The back button's arrow; it points the other way right to left. */
const backArrow = html`<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M15 18l-6-6 6-6" /></svg>`;

const shared = css`
  :host {
    display: block;
    min-block-size: var(--ortus-screen, 100vh);
    color: var(--ortus-text);
  }
  header {
    display: flex;
    align-items: center;
    gap: 12px;
    padding-block: 16px;
    padding-inline: 24px;
    border-block-end: 1px solid var(--ortus-line);
    font-size: 15px;
  }
  header a {
    color: var(--ortus-text);
    text-decoration: none;
    font-weight: 600;
  }
  header span.sep {
    color: var(--ortus-text-3);
  }
  .spacer {
    flex: 1;
  }
  .label {
    padding-block: 3px;
    padding-inline: 8px;
    border-radius: 6px;
    font-family: var(--ortus-mono, monospace);
    font-size: 12px;
    background: color-mix(in srgb, var(--ortus-orange) 18%, transparent);
    color: var(--ortus-orange);
  }
  .card {
    display: grid;
    gap: 14px;
    padding: 24px;
    border-radius: 18px;
    border: 1px solid var(--ortus-line);
    background: var(--ortus-surface);
  }
  .card h2 {
    margin: 0;
    font-size: 17px;
  }
  .error {
    color: var(--ortus-red);
  }
  .back {
    display: none;
  }
  .back svg:dir(rtl) {
    transform: scaleX(-1);
  }
  .scroll {
    overflow-x: auto;
  }
  /* A phone-sized screen: one column; the header wraps. */
  @media (max-width: 720px) {
    header {
      flex-wrap: wrap;
      gap: 8px;
      padding-block: 10px;
      padding-inline: 12px;
    }
    .back {
      display: inline-flex;
      inline-size: 44px;
      padding: 0;
    }
    .button {
      block-size: 44px;
    }
    .card {
      padding: 18px;
    }
  }
`;

@customElement("ortus-records-record")
export class OrtusRecordsRecord extends LitElement {
  static override styles = [
    controls,
    shared,
    css`
      .banner {
        display: flex;
        align-items: center;
        gap: 18px;
        margin-block: 24px 0;
        margin-inline: 24px;
        padding-block: 20px;
        padding-inline: 24px;
        border-radius: 18px;
        border: 1px solid color-mix(in srgb, var(--ortus-teal) 45%, transparent);
        background: color-mix(in srgb, var(--ortus-teal) 10%, transparent);
      }
      .banner i {
        display: grid;
        place-items: center;
        inline-size: 44px;
        block-size: 44px;
        border-radius: 12px;
        background: color-mix(in srgb, var(--ortus-teal) 25%, transparent);
        color: var(--ortus-teal);
        font-style: normal;
        font-size: 20px;
      }
      .banner strong {
        display: block;
        font-size: 17px;
      }
      .banner small {
        color: var(--ortus-text-2);
        font-size: 14px;
      }
      .body {
        display: grid;
        grid-template-columns: minmax(0, 680px) 400px;
        gap: 24px;
        padding: 24px;
        align-items: start;
      }
      .page {
        position: relative;
        border-radius: 6px;
        background: #fafaf8;
        min-block-size: 70vh;
        overflow: hidden;
      }
      .page canvas {
        display: block;
        inline-size: 100%;
        block-size: auto;
      }
      .seal {
        position: absolute;
        inset-block-start: 24px;
        inset-inline-end: 24px;
        padding-block: 4px;
        padding-inline: 12px;
        border: 2px solid #1d7a55;
        border-radius: 8px;
        color: #1d7a55;
        font-weight: 700;
        font-size: 14px;
        rotate: 3deg;
        background: #fafaf8;
      }
      dl {
        display: grid;
        grid-template-columns: auto 1fr;
        gap: 18px 16px;
        margin: 0;
        font-size: 15px;
      }
      dt {
        color: var(--ortus-text-3);
      }
      dd {
        margin: 0;
        text-align: end;
        font-weight: 600;
      }
      dd.mono {
        font-family: var(--ortus-mono, monospace);
        font-weight: 500;
      }
      .valid {
        color: var(--ortus-teal);
      }
      .bad {
        color: var(--ortus-red);
      }
      ul {
        margin: 0;
        padding: 0;
        list-style: none;
        display: grid;
        gap: 12px;
        font-size: 15px;
      }
      aside {
        display: grid;
        gap: 16px;
      }
      .wide {
        inline-size: 100%;
        box-sizing: border-box;
        text-align: center;
      }
      .note {
        color: var(--ortus-text-3);
        font-size: 13px;
        line-height: 1.5;
        margin: 0;
      }
      .checks li {
        display: grid;
        gap: 2px;
      }
      .checks small {
        color: var(--ortus-text-3);
      }
      @media (max-width: 720px) {
        .banner {
          margin-block: 12px 0;
          margin-inline: 12px;
          padding-block: 14px;
          padding-inline: 16px;
        }
        .body {
          grid-template-columns: minmax(0, 1fr);
          gap: 16px;
          padding: 12px;
        }
        .page {
          min-block-size: 0;
        }
      }
    `,
  ];

  private readonly localize = new LocalizeController(this);
  @state() private record: RecordItem | null = null;
  @state() private checks: Check[] | null = null;
  @state() private unchanged = true;
  @state() private showChecks = false;
  @state() private error = "";

  override connectedCallback(): void {
    super.connectedCallback();
    void this.load();
  }

  private recordId(): string {
    return new URLSearchParams(location.search).get("record") ?? "";
  }

  private async load(): Promise<void> {
    try {
      this.record = await call<RecordItem>("GET", `/records/records/${encodeURIComponent(this.recordId())}`);
      const v = await call<{ unchanged: boolean; signatures: Check[] }>("GET", `/records/records/${encodeURIComponent(this.recordId())}/verify`);
      this.checks = v.signatures;
      this.unchanged = v.unchanged;
      await this.updateComplete;
      const page = this.renderRoot.querySelector<HTMLElement>(".canvas");
      if (page) await drawPdf(`/api/v1/records/records/${encodeURIComponent(this.recordId())}/pdf`, page).catch((e) => console.warn("pdf", e));
    } catch (e) {
      this.error = e instanceof ApiError ? e.message : String(e);
    }
  }

  private event(e: { kind: string; detail: string }): string {
    switch (e.kind) {
      case "filed":
        return t(this.localize, "eventFiled", { case: e.detail });
      case "must-read":
        return t(this.localize, "eventMustRead");
      case "posted":
        return t(this.localize, "eventPosted", { channel: e.detail });
      default:
        return e.detail;
    }
  }

  override render() {
    const r = this.record;
    if (!r) return html`${this.error ? html`<p class="error" role="alert">${this.error}</p>` : nothing}`;
    const checks = this.checks ?? [];
    const valid = checks.filter((c) => c.intact && c.trusted).length;
    const last = checks.map((c) => c.at ?? 0).reduce((a, b) => Math.max(a, b), 0);
    const years = Math.round((r.keepUntil - r.filedAt) / (365 * 86_400_000));
    return html`<header>
        <button class="button back" aria-label=${t(this.localize, "back")} @click=${() => location.assign("/records")}>${backArrow}</button>
        <a href="/records">${t(this.localize, "records")}</a><span class="sep">/</span><span>${t(this.localize, "caseNamed", { case: r.case })}</span><span class="sep">/</span><strong>${t(this.localize, "recordNamed", { ref: r.reference })}</strong>
        <span class="spacer"></span>
        ${r.label ? html`<span class="label">${r.label.toUpperCase()}</span>` : nothing}
      </header>
      <section class="banner" role="status">
        <i aria-hidden="true">✓</i>
        <div>
          <strong>${checks.length ? t(this.localize, "signedAndFiled", { count: checks.length, time: hm(r.filedAt) }) : t(this.localize, "filedAt", { time: hm(r.filedAt) })}</strong>
          <small>${t(this.localize, "officialRecord", { title: r.title })}</small>
        </div>
      </section>
      <div class="body">
        <article class="page" aria-label=${r.title}>
          <div class="canvas"></div>
          ${checks.length && valid === checks.length && this.unchanged ? html`<span class="seal">${t(this.localize, "sealed")}</span>` : nothing}
        </article>
        <aside>
          <section class="card" aria-label=${t(this.localize, "record")}>
            <h2>${t(this.localize, "record")}</h2>
            <dl>
              <dt>${t(this.localize, "reference")}</dt><dd class="mono">${r.reference}</dd>
              <dt>${t(this.localize, "type")}</dt><dd>${r.kind === "decision" ? t(this.localize, "decision") : r.kind}</dd>
              <dt>${t(this.localize, "signatures")}</dt>
              <dd class=${this.checks === null ? "" : valid === checks.length && this.unchanged ? "valid" : "bad"}>${this.checks === null ? "…" : t(this.localize, "validOf", { valid, count: checks.length })}</dd>
              ${last ? html`<dt>${t(this.localize, "signedTime")}</dt><dd>${stamp(last)}</dd>` : nothing}
              <dt>${t(this.localize, "keep")}</dt><dd>${t(this.localize, "years", { count: years })}</dd>
            </dl>
          </section>
          <section class="card" aria-label=${t(this.localize, "whatNext")}>
            <h2>${t(this.localize, "whatNext")}</h2>
            <ul>${(r.events ?? []).map((e) => html`<li>${hm(e.at)} · ${this.event(e)}</li>`)}</ul>
          </section>
          <a class="button primary wide" href=${`/api/v1/records/records/${encodeURIComponent(r.id)}/pdf`} download>${t(this.localize, "downloadSigned")}</a>
          <button class="button wide" @click=${() => (this.showChecks = !this.showChecks)}>${t(this.localize, "verifySignatures")}</button>
          ${this.showChecks
            ? html`<section class="card" aria-label=${t(this.localize, "signatures")}>
                <p class=${this.unchanged ? "valid" : "bad"}>${this.unchanged ? t(this.localize, "fileUnchanged") : t(this.localize, "fileChanged")}</p>
                <ul class="checks">
                  ${checks.map(
                    (c) => html`<li>
                      <strong class=${c.intact && c.trusted ? "valid" : "bad"}>${c.intact && c.trusted ? "✓" : "✗"} ${c.signer}</strong>
                      <small>${c.reason}${c.at ? ` · ${stamp(c.at)}` : ""} · ${c.intact ? (c.trusted ? t(this.localize, "checkValid") : t(this.localize, "checkUntrusted")) : t(this.localize, "checkBroken")}${c.revoked_at ? ` · ${t(this.localize, "revokedSince", { date: day(c.revoked_at) })}` : ""}</small>
                    </li>`,
                  )}
                </ul>
                <a href="/api/v1/approvals/authority.pem" download>${t(this.localize, "authorityCert")}</a>
              </section>`
            : nothing}
          <p class="note">${t(this.localize, "cannotChange")}</p>
        </aside>
      </div>`;
  }
}

@customElement("ortus-records-must-read")
export class OrtusRecordsMustRead extends LitElement {
  static override styles = [
    controls,
    shared,
    css`
      .body {
        display: grid;
        grid-template-columns: minmax(0, 720px) 380px;
        gap: 24px;
        padding-block: 24px;
        padding-inline: 124px 24px;
        align-items: start;
      }
      article {
        display: grid;
        align-content: start;
        gap: 16px;
        min-block-size: 70vh;
        padding-block: 48px 32px;
        padding-inline: 56px;
        border-radius: 22px;
        border: 1px solid var(--ortus-line);
        background: var(--ortus-surface);
        font-size: 17px;
        line-height: 1.65;
      }
      article .meta {
        margin: 0;
        color: var(--ortus-text-3);
        font-size: 14px;
      }
      article h1 {
        margin: 0;
        font-size: 30px;
      }
      article h2,
      article h3 {
        margin-block: 8px 0;
      }
      article p {
        margin: 0;
      }
      footer {
        display: flex;
        align-items: center;
        gap: 16px;
        margin-block-start: 24px;
        padding-block-start: 24px;
        border-block-start: 1px solid var(--ortus-line);
        font-size: 15px;
      }
      footer label {
        display: flex;
        gap: 10px;
        align-items: center;
        flex: 1;
      }
      footer input[type="checkbox"] {
        flex: none;
        inline-size: 20px;
        block-size: 20px;
        margin: 0;
        padding: 0;
        accent-color: var(--ortus-accent);
      }
      ol {
        display: grid;
        gap: 8px;
        margin: 0;
        padding: 8px;
        list-style: none;
      }
      ol button {
        display: flex;
        gap: 14px;
        align-items: center;
        inline-size: 100%;
        padding: 14px;
        border-radius: 14px;
        border: 1px solid transparent;
        background: none;
        color: inherit;
        font: inherit;
        text-align: start;
        cursor: pointer;
      }
      ol button[aria-current="true"] {
        border-color: var(--ortus-line-strong);
      }
      ol .n {
        display: grid;
        place-items: center;
        inline-size: 32px;
        block-size: 32px;
        border-radius: 8px;
        background: var(--ortus-surface-2);
        font-weight: 700;
      }
      ol button[aria-current="true"] .n {
        background: var(--ortus-accent);
        color: #0a0b10;
      }
      ol .n.done {
        color: var(--ortus-teal);
      }
      ol strong {
        display: block;
        font-size: 15px;
      }
      ol small,
      .card p {
        color: var(--ortus-text-3);
        font-size: 14px;
        margin: 0;
        line-height: 1.5;
      }
      aside {
        display: grid;
        gap: 16px;
      }
      aside > .card:first-child {
        padding: 0;
      }
      .done-all {
        display: grid;
        gap: 12px;
        justify-items: start;
      }
      @media (max-width: 720px) {
        .body {
          grid-template-columns: minmax(0, 1fr);
          gap: 16px;
          padding: 12px;
        }
        article {
          min-block-size: 0;
          padding-block: 24px;
          padding-inline: 18px;
        }
        article h1 {
          font-size: 24px;
        }
        footer {
          flex-wrap: wrap;
        }
      }
    `,
  ];

  private readonly localize = new LocalizeController(this);
  @state() private items: MustRead[] = [];
  @state() private at = 0;
  @state() private ticked = false;
  @state() private error = "";
  @state() private loaded = false;

  override connectedCallback(): void {
    super.connectedCallback();
    void this.load();
  }

  private async load(): Promise<void> {
    try {
      this.items = (await call<{ items: MustRead[] }>("GET", "/records/must-read")).items;
      const first = this.items.findIndex((i) => !i.confirmedAt);
      this.at = first < 0 ? 0 : first;
    } catch (e) {
      this.error = e instanceof ApiError ? e.message : String(e);
    } finally {
      this.loaded = true;
    }
  }

  private async confirm(): Promise<void> {
    const item = this.items[this.at];
    if (!item) return;
    try {
      this.items = (await call<{ items: MustRead[] }>("POST", `/records/must-read/${encodeURIComponent(item.id)}/confirm`)).items;
      this.ticked = false;
      const next = this.items.findIndex((i) => !i.confirmedAt);
      if (next >= 0) this.at = next;
      else {
        // All read: the welcome page's step is done.
        await call("POST", "/me/welcome/policies").catch(() => undefined);
        this.at = this.items.length;
      }
    } catch (e) {
      this.error = e instanceof ApiError ? e.message : String(e);
    }
  }

  override render() {
    const item = this.items[this.at];
    const left = this.items.filter((i) => !i.confirmedAt).length;
    return html`<header>
        <a href="/welcome">${t(this.localize, "welcome")}</a><span class="sep">/</span><span>${t(this.localize, "readPolicies", { count: this.items.length })}</span>
        <span class="spacer"></span>
        ${item ? html`<small>${t(this.localize, "policyOf", { n: this.at + 1, count: this.items.length })}</small>` : nothing}
      </header>
      ${this.error ? html`<p class="error" role="alert">${this.error}</p>` : nothing}
      <div class="body">
        ${item
          ? html`<article aria-label=${item.title}>
              ${item.note ? html`<p class="meta">${item.note}</p>` : nothing}
              <h1>${item.title}</h1>
              ${(item.body.content ?? []).map(block)}
              <footer>
                ${item.confirmedAt
                  ? html`<span>${t(this.localize, "confirmedOn", { date: stamp(item.confirmedAt) })}</span>`
                  : html`<label><input type="checkbox" .checked=${this.ticked} @change=${(e: Event) => (this.ticked = (e.target as HTMLInputElement).checked)} />${t(this.localize, "readAndUnderstood")}</label>
                      <button class="button primary" ?disabled=${!this.ticked} @click=${this.confirm}>${left > 1 ? t(this.localize, "confirmNext") : t(this.localize, "confirm")}</button>`}
              </footer>
            </article>`
          : this.loaded
            ? html`<article class="done-all" role="status"><h1>${this.items.length ? t(this.localize, "allConfirmed") : t(this.localize, "nothingToRead")}</h1><a class="button primary" href="/welcome">${t(this.localize, "backToWelcome")}</a></article>`
            : html`<article></article>`}
        <aside>
          <section class="card" aria-label=${t(this.localize, "policies")}>
            <ol>
              ${this.items.map(
                (i, n) => html`<li>
                  <button aria-current=${n === this.at ? "true" : "false"} @click=${() => ((this.at = n), (this.ticked = false))}>
                    <span class="n ${i.confirmedAt ? "done" : ""}">${i.confirmedAt ? "✓" : n + 1}</span>
                    <span><strong>${i.title}</strong><small>${t(this.localize, "aboutMinutes", { count: i.minutes })}${n === this.at ? ` · ${t(this.localize, "readingNow")}` : i.confirmedAt ? ` · ${t(this.localize, "confirmedWord")}` : ""}</small></span>
                  </button>
                </li>`,
              )}
            </ol>
          </section>
          <section class="card">
            <h2>${t(this.localize, "whatConfirming")}</h2>
            <p>${t(this.localize, "confirmingMeans")}</p>
          </section>
        </aside>
      </div>`;
  }
}

@customElement("ortus-records-home")
export class OrtusRecordsHome extends LitElement {
  static override styles = [
    controls,
    shared,
    css`
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
        text-align: start;
        border-block-end: 1px solid var(--ortus-line);
      }
      th {
        color: var(--ortus-text-3);
        font-weight: 500;
      }
      td a {
        color: var(--ortus-text);
        font-weight: 600;
        text-decoration: none;
      }
      .mono {
        font-family: var(--ortus-mono, monospace);
      }
      .empty {
        margin: 48px;
        color: var(--ortus-text-3);
      }
      @media (max-width: 720px) {
        table {
          inline-size: max-content;
          min-inline-size: calc(100% - 24px);
          margin: 12px;
        }
        td a {
          display: inline-block;
          padding-block: 12px;
        }
        .empty {
          margin: 24px;
        }
      }
    `,
  ];

  private readonly localize = new LocalizeController(this);
  @state() private list: RecordItem[] = [];
  @state() private loaded = false;

  override connectedCallback(): void {
    super.connectedCallback();
    void call<{ records: RecordItem[] }>("GET", "/records/records")
      .then((r) => (this.list = r.records))
      .finally(() => (this.loaded = true));
  }

  override render() {
    return html`<header><strong>${t(this.localize, "records")}</strong><span class="spacer"></span><a class="button" href="/records?read=1">${t(this.localize, "mustRead")}</a></header>
      ${this.list.length
        ? html`<div class="scroll"><table>
            <thead><tr><th>${t(this.localize, "reference")}</th><th>${t(this.localize, "titleWord")}</th><th>${t(this.localize, "caseWord")}</th><th>${t(this.localize, "filedWord")}</th><th>${t(this.localize, "keepUntil")}</th></tr></thead>
            <tbody>
              ${this.list.map((r) => html`<tr><td class="mono">${r.reference}</td><td><a href=${`/records?record=${encodeURIComponent(r.id)}`}>${r.title}</a></td><td class="mono">${r.case}</td><td>${stamp(r.filedAt)}</td><td>${day(r.keepUntil)}</td></tr>`)}
            </tbody>
          </table></div>`
        : this.loaded
          ? html`<p class="empty" role="status">${t(this.localize, "noRecords")}</p>`
          : nothing}`;
  }
}
