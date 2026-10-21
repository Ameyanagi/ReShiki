import type { FontSource, MountOptions } from "./editor.js";

/** The default Japanese fallback: Noto Sans JP Regular on jsDelivr. */
export const NOTO_SANS_JP: string;

/** The page's globals these functions use; tests pass their own. */
export interface FontPage {
  navigator?: { permissions?: { query(descriptor: { name: string }): Promise<{ state: string }> } };
  queryLocalFonts?: () => Promise<{ family: string; style: string; blob(): Promise<Blob> }[]>;
  fetch?: typeof fetch;
}

/** The bytes of a font URL, or the bytes given. */
export function fontBytes(font: FontSource, fetchFont?: typeof fetch): Promise<Uint8Array>;

/** The best Japanese system font's regular and bold faces when the page
 * already has Local Font Access permission; none otherwise. */
export function systemCjkFonts(page?: FontPage): Promise<Uint8Array[]>;

/** The Japanese fallback: system fonts when allowed, else `cjkFont` (Noto Sans
 * JP by default) downloaded; nothing when it is false. */
export function cjkFonts(cjkFont?: MountOptions["cjkFont"], page?: FontPage): Promise<Uint8Array[]>;

/** Add `options.fonts` to the editor and ask for the Japanese fallback when it
 * first needs one. */
export function loadFonts(
  wasm: {
    addFont(bytes: Uint8Array): number;
    onMissingFont(callback?: () => void): void;
  },
  options?: Pick<MountOptions, "fonts" | "cjkFont" | "onError">,
  page?: FontPage,
): Promise<void>;
