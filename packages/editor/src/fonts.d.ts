import type { FontSource, MountOptions } from "./editor.js";

/** The languages the editor asks for a fallback font in. */
export type CjkLanguage = "ja" | "ko" | "zh";

/** The default Japanese fallback: Noto Sans JP Regular on jsDelivr. */
export const NOTO_SANS_JP: string;

/** The default fallback for each language: Noto Sans JP, KR and SC Regular
 * on jsDelivr. */
export const NOTO_SANS: Record<CjkLanguage, string>;

/** The page's globals these functions use; tests pass their own. */
export interface FontPage {
  navigator?: { permissions?: { query(descriptor: { name: string }): Promise<{ state: string }> } };
  queryLocalFonts?: () => Promise<{ family: string; style: string; blob(): Promise<Blob> }[]>;
  fetch?: typeof fetch;
}

/** The bytes of a font URL, or the bytes given. */
export function fontBytes(font: FontSource, fetchFont?: typeof fetch): Promise<Uint8Array>;

/** The best system font for `language` (Japanese by default), its regular
 * and bold faces, when the page already has Local Font Access permission;
 * none otherwise. */
export function systemCjkFonts(page?: FontPage, language?: CjkLanguage): Promise<Uint8Array[]>;

/** The fallback for `language`: system fonts when allowed, else `cjkFont`
 * (the language's Noto Sans subset by default) downloaded; nothing when it is
 * false. */
export function cjkFonts(
  cjkFont?: MountOptions["cjkFont"],
  page?: FontPage,
  language?: CjkLanguage,
): Promise<Uint8Array[]>;

/** Add `options.fonts` to the editor, and return the callback that loads a
 * language's fallback when the editor first needs one (undefined for none). */
export function loadFonts(
  wasm: { addFont(bytes: Uint8Array): number },
  options?: Pick<MountOptions, "fonts" | "cjkFont" | "onError">,
  page?: FontPage,
): Promise<((language?: CjkLanguage) => Promise<void>) | undefined>;
