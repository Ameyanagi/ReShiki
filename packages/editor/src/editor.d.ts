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
}

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
