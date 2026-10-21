// The font loader with a stand-in page and editor: which fonts the Japanese
// fallback loads, and how host fonts and failures are handled.
import { describe, expect, test } from "bun:test";
import { cjkFonts, loadFonts, NOTO_SANS_JP } from "../src/fonts.js";
import type { FontPage } from "../src/fonts.js";

const bytes = (...values: number[]) => new Uint8Array(values);
const face = (family: string, style: string, value: number) => ({
  family,
  style,
  blob: async () => new Blob([bytes(value)]),
});

/** A page with Local Font Access in `state`, its system fonts, and a fetch
 * that records each URL and answers with `[9]`, or 404 for `missing`. */
function page(state: string | undefined, fonts = [face("Hiragino Sans", "W3", 1)]) {
  const fetched: string[] = [];
  const queried: number[] = [];
  const result: FontPage & { fetched: string[]; queried: number[] } = {
    fetched,
    queried,
    navigator: state === undefined ? {} : { permissions: { query: async () => ({ state }) } },
    queryLocalFonts: async () => {
      queried.push(1);
      return fonts;
    },
    fetch: (async (url: string | URL) => {
      fetched.push(String(url));
      return String(url).includes("missing")
        ? new Response(null, { status: 404 })
        : new Response(bytes(9));
    }) as typeof fetch,
  };
  return result;
}

describe("Japanese fallback", () => {
  test("uses system fonts when Local Font Access is already granted", async () => {
    const granted = page("granted", [
      face("Arial", "Regular", 0),
      face("Hiragino Sans", "W1", 1),
      face("Hiragino Sans", "W3", 3),
      face("Hiragino Sans", "W6", 6),
      face("Yu Gothic", "Regular", 7),
    ]);
    expect(await cjkFonts(undefined, granted)).toEqual([bytes(3), bytes(6)]);
    expect(granted.fetched).toEqual([]);
  });

  test("downloads Noto Sans JP when permission is not granted, without asking", async () => {
    for (const state of ["prompt", "denied"]) {
      const asked = page(state);
      expect(await cjkFonts(undefined, asked)).toEqual([bytes(9)]);
      expect(asked.fetched).toEqual([NOTO_SANS_JP]);
      // queryLocalFonts would show the permission prompt.
      expect(asked.queried).toEqual([]);
    }
  });

  test("downloads when the browser has no Local Font Access", async () => {
    const plain = page(undefined);
    delete plain.queryLocalFonts;
    expect(await cjkFonts("https://fonts.test/jp.otf", plain)).toEqual([bytes(9)]);
    expect(plain.fetched).toEqual(["https://fonts.test/jp.otf"]);
    const unknown = page(undefined);
    unknown.navigator = {
      permissions: {
        query: async () => {
          throw new TypeError("local-fonts is not a permission name");
        },
      },
    };
    expect(await cjkFonts(undefined, unknown)).toEqual([bytes(9)]);
  });

  test("downloads when no Japanese system font is installed", async () => {
    const latin = page("granted", [face("Arial", "Regular", 0)]);
    expect(await cjkFonts(undefined, latin)).toEqual([bytes(9)]);
  });

  test("cjkFont: false loads nothing", async () => {
    const granted = page("granted");
    expect(await cjkFonts(false, granted)).toEqual([]);
    expect(granted.fetched).toEqual([]);
    expect(granted.queried).toEqual([]);
  });
});

/** A stand-in editor that records added fonts and its missing-font callback. */
function editor() {
  const added: Uint8Array[] = [];
  const state: { missing?: () => void } = {};
  return {
    added,
    state,
    addFont(font: Uint8Array) {
      if (font[0] === 0) throw new Error("Not a TrueType or OpenType font");
      added.push(font);
      return 1;
    },
    onMissingFont(callback?: () => void) {
      state.missing = callback;
    },
  };
}

describe("loadFonts", () => {
  test("adds host fonts before the editor starts and reports failures", async () => {
    const wasm = editor();
    const errors: string[] = [];
    const host = page("prompt");
    await loadFonts(
      wasm,
      {
        fonts: ["https://fonts.test/a.ttf", "https://fonts.test/missing.ttf", bytes(5), bytes(0)],
        onError: (error) => errors.push(error.message),
      },
      host,
    );
    expect(wasm.added).toEqual([bytes(9), bytes(5)]);
    expect(errors).toEqual([
      "Could not load the font https://fonts.test/missing.ttf: HTTP 404",
      "Could not add a font: Not a TrueType or OpenType font",
    ]);
    // The Japanese fallback waits until the editor asks.
    expect(host.fetched).toEqual(["https://fonts.test/a.ttf", "https://fonts.test/missing.ttf"]);
  });

  test("loads the Japanese fallback when the editor first needs it", async () => {
    const wasm = editor();
    const host = page("denied");
    await loadFonts(wasm, {}, host);
    expect(host.fetched).toEqual([]);
    await wasm.state.missing?.();
    expect(host.fetched).toEqual([NOTO_SANS_JP]);
    expect(wasm.added).toEqual([bytes(9)]);
  });

  test("reports a fallback that cannot be downloaded", async () => {
    const wasm = editor();
    const errors: string[] = [];
    await loadFonts(
      wasm,
      { cjkFont: "https://fonts.test/missing.otf", onError: (error) => errors.push(error.message) },
      page("denied"),
    );
    await wasm.state.missing?.();
    expect(errors).toEqual(["Could not load the font https://fonts.test/missing.otf: HTTP 404"]);
  });

  test("cjkFont: false never asks", async () => {
    const wasm = editor();
    await loadFonts(wasm, { cjkFont: false }, page("granted"));
    expect(wasm.state.missing).toBeUndefined();
  });
});
