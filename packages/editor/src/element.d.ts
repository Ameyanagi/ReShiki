import type { Editor } from "./editor.js";

/** `<reshiki-editor src="…" format="…">`, defined by importing
 * `@ameyanagi/reshiki/element`. Fires `ready` and `change`. */
export class ReshikiEditor extends HTMLElement {
  /** Resolves once the editor runs. */
  readonly ready: Promise<Editor>;
  load: Editor["load"];
  save: Editor["save"];
  exportImage: Editor["exportImage"];
}

declare global {
  interface HTMLElementTagNameMap {
    "reshiki-editor": ReshikiEditor;
  }
}
