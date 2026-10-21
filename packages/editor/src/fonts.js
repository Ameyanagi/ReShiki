// Fonts for the editor, which can only draw with fonts loaded into it as
// bytes. The page's own fonts load before the editor starts. For Japanese
// (CJK) text the editor asks once for a fallback: the computer's own font when
// the page may already read local fonts, so no prompt appears, or else
// Noto Sans JP, downloaded once.

/** Noto Sans JP Regular (SIL Open Font License), pinned to release Sans2.004. */
export const NOTO_SANS_JP =
  "https://cdn.jsdelivr.net/gh/notofonts/noto-cjk@Sans2.004/Sans/SubsetOTF/JP/NotoSansJP-Regular.otf";

// Japanese system fonts, best first: macOS, Windows, then Linux.
const SYSTEM_CJK = [
  "Hiragino Sans",
  "Hiragino Kaku Gothic ProN",
  "Yu Gothic",
  "Meiryo",
  "Noto Sans CJK JP",
  "Noto Sans JP",
];
const REGULAR_OR_BOLD = /^(regular|normal|w3|bold|w6)$/i;

/** The bytes of `font`: a URL to fetch, or the bytes themselves. */
export async function fontBytes(font, fetchFont = fetch) {
  if (typeof font !== "string" && !(font instanceof URL)) {
    return font instanceof ArrayBuffer ? new Uint8Array(font) : font;
  }
  let response;
  try {
    response = await fetchFont(font);
  } catch (error) {
    throw new Error(`Could not load the font ${font}: ${error?.message ?? error}`);
  }
  if (!response.ok) throw new Error(`Could not load the font ${font}: HTTP ${response.status}`);
  return new Uint8Array(await response.arrayBuffer());
}

/** The regular and bold faces of the best Japanese system font, when the
 * page already has Local Font Access; none otherwise, without a prompt. */
export async function systemCjkFonts(page = globalThis) {
  try {
    const status = await page.navigator?.permissions?.query({ name: "local-fonts" });
    if (status?.state !== "granted" || typeof page.queryLocalFonts !== "function") return [];
    const fonts = await page.queryLocalFonts();
    for (const family of SYSTEM_CJK) {
      const faces = fonts.filter((font) => font.family === family);
      const chosen = faces.filter((font) => REGULAR_OR_BOLD.test(font.style));
      const use = chosen.length ? chosen : faces.slice(0, 1);
      if (use.length) {
        return await Promise.all(
          use.map(async (font) => new Uint8Array(await (await font.blob()).arrayBuffer())),
        );
      }
    }
  } catch {
    // Browsers without the API or the permission name throw; download instead.
  }
  return [];
}

/** The Japanese fallback's bytes: system fonts when allowed, else `cjkFont`
 * (Noto Sans JP by default) downloaded; nothing when `cjkFont` is false. */
export async function cjkFonts(cjkFont, page = globalThis) {
  if (cjkFont === false) return [];
  const system = await systemCjkFonts(page);
  if (system.length) return system;
  return [await fontBytes(cjkFont ?? NOTO_SANS_JP, page.fetch?.bind(page) ?? fetch)];
}

/** Add the page's `fonts` now, and ask for the Japanese fallback when the
 * editor first needs it. `wasm` has the editor's addFont and onMissingFont;
 * failures go to `onError`. */
export async function loadFonts(wasm, options = {}, page = globalThis) {
  const onError = options.onError ?? ((error) => console.error(error));
  const add = (bytes) => {
    try {
      wasm.addFont(bytes);
    } catch (error) {
      onError(new Error(`Could not add a font: ${error?.message ?? error}`));
    }
  };
  const fetchFont = page.fetch?.bind(page) ?? fetch;
  const loaded = await Promise.allSettled(
    (options.fonts ?? []).map((font) => fontBytes(font, fetchFont)),
  );
  for (const result of loaded) {
    if (result.status === "fulfilled") add(result.value);
    else onError(result.reason);
  }
  wasm.onMissingFont(
    options.cjkFont === false
      ? undefined
      : () =>
          cjkFonts(options.cjkFont, page).then(
            (fonts) => fonts.forEach(add),
            (error) => onError(error),
          ),
  );
}
