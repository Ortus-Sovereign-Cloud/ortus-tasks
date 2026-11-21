// The look of the start page's cards, shared by this repository's apps.
import { css } from "ortus-kit";

export const cardStyles = css`
  :host {
    display: block;
  }
  section {
    padding: 20px;
    border-radius: 18px;
    border: 1px solid var(--ortus-line);
    background: var(--ortus-surface);
  }
  header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    margin-block-end: 12px;
  }
  h2 {
    margin: 0;
    font-size: 17px;
    font-weight: 600;
  }
  .count {
    color: var(--ortus-accent);
    font-size: 14px;
    font-weight: 600;
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li a {
    display: flex;
    flex-direction: column;
    padding: 8px;
    margin-inline: -8px;
    border-radius: 10px;
    color: inherit;
    text-decoration: none;
  }
  li a:hover,
  li a:focus-visible {
    background: var(--ortus-hover);
  }
  li strong {
    font-size: 14px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  li small {
    color: var(--ortus-text-2);
    font-size: 13px;
  }
  .late {
    color: var(--ortus-red);
  }
  .quiet {
    margin: 0;
    color: var(--ortus-text-2);
    font-size: 14px;
  }
  footer {
    margin-block-start: 12px;
    font-size: 14px;
  }
  footer a {
    color: var(--ortus-accent);
  }
`;
