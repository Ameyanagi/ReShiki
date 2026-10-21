import type { Editor } from "./editor.js";

export interface EmbedOptions {
  /** The frame page; this package's `src/frame.html` by default. It must be
   * served with the package's `src` and `dist` folders beside it. */
  frameUrl?: string | URL;
  /** The iframe's accessible title. */
  title?: string;
  /** Called after each edit. */
  onChange?: () => void;
  /** Font URLs, as for mount(); relative URLs resolve against this page. */
  fonts?: (string | URL)[];
  /** The Japanese fallback's URL, or `false` for none, as for mount(). */
  cjkFont?: string | URL | false;
  /** Called with fonts that could not be loaded; they are logged by default. */
  onError?: (error: Error) => void;
  /** Milliseconds to wait for the frame to start before `ready` and waiting
   * calls reject; 60000 by default. */
  timeout?: number;
}

export interface EmbeddedEditor extends Editor {
  /** Resolves once the editor in the frame runs. */
  readonly ready: Promise<void>;
}

/** Put the editor in an iframe inside `element`, which it fills. Each frame
 * has its own editor, so a page can embed several. */
export function embed(element: HTMLElement, options?: EmbedOptions): EmbeddedEditor;
