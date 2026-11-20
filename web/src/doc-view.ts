// A document's structure (ProseMirror JSON) as plain, read-only HTML.
import { html, nothing, type TemplateResult } from "ortus-kit";

export interface PMNode {
  type: string;
  text?: string;
  attrs?: { level?: number };
  marks?: { type: string }[];
  content?: PMNode[];
}

function inline(nodes: PMNode[] = []): (TemplateResult | string)[] {
  return nodes.map((n) => {
    if (n.type !== "text" || n.marks?.some((m) => m.type === "insertion")) return "";
    let out: TemplateResult | string = n.text ?? "";
    if (n.marks?.some((m) => m.type === "strong")) out = html`<strong>${out}</strong>`;
    if (n.marks?.some((m) => m.type === "em")) out = html`<em>${out}</em>`;
    return out;
  });
}

function text(n: PMNode): string {
  return (n.text ?? "") + (n.content ?? []).map(text).join("");
}

export function block(node: PMNode): TemplateResult | typeof nothing {
  switch (node.type) {
    case "heading":
      return node.attrs?.level === 1 ? html`<h2>${inline(node.content)}</h2>` : html`<h3>${inline(node.content)}</h3>`;
    case "paragraph":
      return html`<p>${inline(node.content)}</p>`;
    case "bullet_list":
      return html`<ul>${(node.content ?? []).map((li) => html`<li>${(li.content ?? []).map(block)}</li>`)}</ul>`;
    case "ordered_list":
      return html`<ol>${(node.content ?? []).map((li) => html`<li>${(li.content ?? []).map(block)}</li>`)}</ol>`;
    case "table":
      return html`<table>${(node.content ?? []).map((row) => html`<tr>${(row.content ?? []).map((cell) => (cell.type === "table_header" ? html`<th>${text(cell)}</th>` : html`<td>${text(cell)}</td>`))}</tr>`)}</table>`;
    default:
      return nothing;
  }
}
