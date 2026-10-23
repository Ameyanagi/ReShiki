import type { Editor } from "./editor.js";

/** `<reshiki-editor src="…" format="…" fonts="…" cjk-font="…">`, defined by
 * importing `@ameyanagi/reshiki/element`. `fonts` lists font URLs separated by
 * spaces; `cjk-font` is the CJK fallback's URL, or `false` for none.
 * Fires `ready`, `change`, and `error` for a font that could not load. */
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
