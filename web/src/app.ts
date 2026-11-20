// The tasks client: the element the shell shows at /tasks.
import { LitElement, css, customElement, html } from "ortus-kit";
import "./board.js";
import { ready } from "./i18n.js";

await ready;

@customElement("ortus-tasks-app")
export class OrtusTasksApp extends LitElement {
  static override styles = css`
    :host {
      display: block;
    }
  `;

  override render() {
    return html`<ortus-tasks-board></ortus-tasks-board>`;
  }
}
