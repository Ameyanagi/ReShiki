import type { Input, InputFormat, OutputFormat, BinaryFormat, TextFormat } from "./index.js";

/** Figures as the desktop's Export makes them. */
export type ImageFormat = "svg" | "png" | "pdf";

export interface MountOptions {
  /** Where to load the editor's WebAssembly from; next to this package's
   * `dist/editor/reshiki_bg.wasm` by default. */
  module?: string | URL | Request | Response | BufferSource | WebAssembly.Module;
  /** A drawing or structure to open. */
  data?: Input;
  /** The format of `data`; `auto` by default. */
  format?: InputFormat;
  /** Called after each edit to the drawing in front. */
  onChange?: () => void;
  /** Called with `true` when a tab gains unsaved changes and `false` when
   * none has any left. */
  onDirtyChange?: (dirty: boolean) => void;
  /** Called with the title, `Name • — ReShiki` while it has unsaved
   * changes, whenever it changes. */
  onTitleChange?: (title: string) => void;
  /** Make the title the page's `document.title`, as the desktop window's
   * is; `false` by default, so the page keeps its own. */
  title?: boolean;
  /** TrueType or OpenType fonts (or collections) to draw with, as URLs or
   * bytes. They load before the editor starts; text whose font lacks a glyph
   * falls back to them. URLs on another origin need CORS. */
  fonts?: FontSource[];
  /** The Chinese, Japanese and Korean (CJK) fallback, loaded the first time
   * a caption, tab name or the status line needs one when no other font has
   * its glyphs.
   * The computer's own font for that language is used when the page already
   * has Local Font Access permission; otherwise this URL is downloaded once
   * for every language, or by default the language's Noto Sans subset (JP, KR
   * or SC) from jsDelivr. `false` loads nothing. */
  cjkFont?: string | URL | false;
  /** Called with fonts that could not be loaded; they are logged by default. */
  onError?: (error: Error) => void;
}

/** A font's URL, or its bytes. */
export type FontSource = string | URL | Uint8Array | ArrayBuffer;

export interface Editor {
  /** Replace the drawing in front. Rejects with the app's message when the
   * data cannot be read. */
  load(data: Input, format?: InputFormat): Promise<void>;
  /** The drawing in front; `rsk` (a native drawing) by default. */
  save(format: BinaryFormat): Promise<Uint8Array>;
  save(format?: TextFormat): Promise<string>;
  save(format: OutputFormat): Promise<string | Uint8Array>;
  /** The drawing in front as a figure; `svg` by default. */
  exportImage(format?: "svg"): Promise<string>;
  exportImage(format: "png" | "pdf"): Promise<Uint8Array>;
  exportImage(format: ImageFormat): Promise<string | Uint8Array>;
  /** Call `callback` after each edit; returns a function that stops it. */
  onChange(callback: () => void): () => void;
  /** Whether any tab has unsaved changes. Leaving the page then asks first. */
  isDirty(): boolean;
  /** Call `callback` when `isDirty()` changes; returns a function that stops it. */
  onDirtyChange(callback: (dirty: boolean) => void): () => void;
  /** Call `callback` with the title when it changes; returns a function that
   * stops it. */
  onTitleChange(callback: (title: string) => void): () => void;
  /** Stop the editor and empty its element. A page cannot mount another
   * editor afterwards; reload it, or use embed() for several editors. */
  destroy(): void;
}

/** Load the editor's WebAssembly; mount() calls it. */
export function initEditor(module?: MountOptions["module"]): Promise<void>;

/** Start the editor in `element`, which it fills. A page can mount one
 * editor; a second mount rejects. Data that cannot be read rejects before the
 * editor starts, so the page can mount again. */
export function mount(element: HTMLElement, options?: MountOptions): Promise<Editor>;
