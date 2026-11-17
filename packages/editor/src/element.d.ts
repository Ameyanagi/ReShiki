import type { Editor } from "./editor.js";

/** `<reshiki-editor src="…" format="…" fonts="…" cjk-font="…">`, defined by
 * importing `@ameyanagi/reshiki/element`. `fonts` lists font URLs separated by
 * spaces; `cjk-font` is the CJK fallback's URL, or `false` for none.
 * Fires `ready`, `change`, and `error` for a font that could not load or an
 * editor that could not start, such as a `src` that answers 404.
 *
 * Changing `src` or `format` once the editor runs loads the drawing at `src`
 * in place of the one in front. Once the file is fetched it fires a cancelable
 * `beforeload` {@link BeforeLoadEvent}; `preventDefault()` keeps the current
 * drawing. A `src` that cannot be loaded fires `error` and keeps it too. */
export class ReshikiEditor extends HTMLElement {
  /** Resolves once the editor runs. */
  readonly ready: Promise<Editor>;
  load: Editor["load"];
  save: Editor["save"];
  exportImage: Editor["exportImage"];
}

/** Fired before a changed `src` or `format` replaces the drawing in front. */
export type BeforeLoadEvent = CustomEvent<{
  /** The new `src` attribute. */
  src: string;
  /** The new `format` attribute; `auto` without one. */
  format: string;
  /** Whether any tab has unsaved changes, as `isDirty()` reports. */
  dirty: boolean;
}>;

declare global {
  interface HTMLElementTagNameMap {
    "reshiki-editor": ReshikiEditor;
  }
}
