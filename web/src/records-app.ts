// The records client: the element the shell shows at /records — a record
// (?record=), the must-read policies (?read=1) or the list.
import { LitElement, css, customElement, html } from "ortus-kit";
import { ready } from "./i18n.js";
import "./records.js";

await ready;

@customElement("ortus-records-app")
export class OrtusRecordsApp extends LitElement {
  static override styles = css`
    :host {
      display: block;
    }
  `;

  override render() {
    const p = new URLSearchParams(location.search);
    if (p.get("record")) return html`<ortus-records-record></ortus-records-record>`;
    if (p.get("read")) return html`<ortus-records-must-read></ortus-records-must-read>`;
    return html`<ortus-records-home></ortus-records-home>`;
  }
}
