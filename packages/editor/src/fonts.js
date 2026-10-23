// Fonts for the editor, which can only draw with fonts loaded into it as
// bytes. The page's own fonts load before the editor starts. For Chinese,
// Japanese and Korean text the editor asks once per language for a fallback:
// the computer's own font when the page may already read local fonts, so no
// prompt appears, or else a Noto Sans subset, downloaded once.

const NOTO_CJK = "https://cdn.jsdelivr.net/gh/notofonts/noto-cjk@Sans2.004/Sans/SubsetOTF";

/** Noto Sans JP Regular (SIL Open Font License), pinned to release Sans2.004. */
export const NOTO_SANS_JP = `${NOTO_CJK}/JP/NotoSansJP-Regular.otf`;

/** The Noto Sans subset for each language the editor asks for. */
export const NOTO_SANS = {
  ja: NOTO_SANS_JP,
  ko: `${NOTO_CJK}/KR/NotoSansKR-Regular.otf`,
  zh: `${NOTO_CJK}/SC/NotoSansSC-Regular.otf`,
};

// System fonts for each language, best first: macOS, Windows, then Linux.
const SYSTEM_CJK = {
  ja: [
    "Hiragino Sans",
    "Hiragino Kaku Gothic ProN",
    "Yu Gothic",
    "Meiryo",
    "Noto Sans CJK JP",
    "Noto Sans JP",
  ],
  ko: ["Apple SD Gothic Neo", "Malgun Gothic", "Noto Sans CJK KR", "Noto Sans KR"],
  zh: ["PingFang SC", "Microsoft YaHei", "Noto Sans CJK SC", "Noto Sans SC"],
};
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

/** The regular and bold faces of the best system font for `language` ("ja",
 * "ko" or "zh"), when the page already has Local Font Access; none
 * otherwise, without a prompt. */
export async function systemCjkFonts(page = globalThis, language = "ja") {
  try {
    const status = await page.navigator?.permissions?.query({ name: "local-fonts" });
    if (status?.state !== "granted" || typeof page.queryLocalFonts !== "function") return [];
    const fonts = await page.queryLocalFonts();
    for (const family of SYSTEM_CJK[language] ?? SYSTEM_CJK.ja) {
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

/** The fallback's bytes for `language`: system fonts when allowed, else
 * `cjkFont` downloaded, or the language's Noto Sans subset by default;
 * nothing when `cjkFont` is false. */
export async function cjkFonts(cjkFont, page = globalThis, language = "ja") {
  if (cjkFont === false) return [];
  const system = await systemCjkFonts(page, language);
  if (system.length) return system;
  const url = cjkFont ?? NOTO_SANS[language] ?? NOTO_SANS_JP;
  return [await fontBytes(url, page.fetch?.bind(page) ?? fetch)];
}

/** Add the page's `fonts` to the editor (`wasm.addFont`) now, and return the
 * callback the editor calls with a language ("ja", "ko" or "zh") when it
 * first needs that language's fallback, or undefined for none. A `cjkFont`
 * the page chose serves every language and loads once. Failures go to
 * `onError`. */
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
  if (options.cjkFont === false) return undefined;
  const requested = new Set();
  let chosenLoaded = false;
  const load = async (language) => {
    const system = await systemCjkFonts(page, language);
    if (system.length) return system;
    if (options.cjkFont === undefined) {
      return [await fontBytes(NOTO_SANS[language] ?? NOTO_SANS_JP, fetchFont)];
    }
    // The page's own fallback serves every language, so it loads once.
    if (chosenLoaded) return [];
    chosenLoaded = true;
    return [await fontBytes(options.cjkFont, fetchFont)];
  };
  return (language = "ja") => {
    if (requested.has(language)) return Promise.resolve();
    requested.add(language);
    return load(language).then(
      (fonts) => fonts.forEach(add),
      (error) => onError(error),
    );
  };
}
