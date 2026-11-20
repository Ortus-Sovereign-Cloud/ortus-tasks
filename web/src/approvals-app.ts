// The approvals client: the element the shell shows at /approvals.
import { LitElement, css, customElement, html, state } from "ortus-kit";
import "./approvals.js";
import { ready } from "./i18n.js";

await ready;

function review(): string | null {
  return new URLSearchParams(location.search).get("review");
}

@customElement("ortus-approvals-app")
export class OrtusApprovalsApp extends LitElement {
  static override styles = css`
    :host {
      display: block;
    }
  `;

  @state() private docsReady = false;

  override connectedCallback(): void {
    super.connectedCallback();
    // A route is shown by the docs app's review element, loaded from it.
    if (review()) {
      const url = "/assets/apps/docs/app.js";
      void import(/* @vite-ignore */ url).then(() => (this.docsReady = true));
    }
  }

  override render() {
    if (review()) return this.docsReady ? html`<ortus-docs-review></ortus-docs-review>` : html``;
    return html`<ortus-approvals-home></ortus-approvals-home>`;
  }
}
