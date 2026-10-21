// Loads the built wasm (scripts/build_wasm.sh) and checks it against the
// desktop. Set RESHIKI_CLI to a native `reshiki` binary to compare outputs
// with `reshiki --cli`; without it those tests are skipped.
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import {
  addFont,
  convert,
  init,
  inchi,
  renderPng,
  renderSvg,
  smiles,
  version,
} from "../src/index.js";

const CAFFEINE = "CN1C=NC2=C1C(=O)N(C(=O)N2C)C";
// Rings, stereo centres, a double-bond geometry, charges and a salt.
const STRUCTURES = [
  CAFFEINE,
  "CC(=O)Oc1ccccc1C(=O)O",
  "C[C@@H](N)C(=O)O",
  "C/C=C/C(=O)[O-].[Na+]",
  "c1ccc2cc3ccccc3cc2c1",
  "CC(C)CCC[C@@H](C)[C@H]1CC[C@H]2[C@@H]3CC=C4C[C@@H](O)CC[C@]4(C)[C@H]3CC[C@]12C",
];
const PNG = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

// A failed load is not cached: init succeeds after it.
await expect(init(new Uint8Array([0]))).rejects.toThrow();
await init(readFileSync(new URL("../dist/wasm/reshiki_web_bg.wasm", import.meta.url)));

describe("headless library", () => {
  test("caffeine round-trips SMILES -> MOL -> SMILES", async () => {
    const mol = await convert(CAFFEINE, "smiles", "mol");
    expect(mol).toContain("M  END");
    expect(await convert(mol, "mol", "smiles")).toBe(await smiles(CAFFEINE, "smiles"));
  });

  test("InChI runs in-process", async () => {
    expect(await inchi(CAFFEINE, "smiles")).toBe(
      "InChI=1S/C8H10N4O2/c1-10-4-9-6-5(10)7(13)12(3)8(14)11(6)2/h4H,1-3H3",
    );
    const read = await convert(await inchi(CAFFEINE, "smiles"), "inchi", "smiles");
    expect(read).toBe(await smiles(CAFFEINE, "smiles"));
  });

  test("native drawings and CDX round-trip", async () => {
    const expected = await smiles(CAFFEINE, "smiles");
    const rsk = await convert(CAFFEINE, "smiles", "rsk");
    expect(JSON.parse(rsk).version).toBeNumber();
    expect(await smiles(rsk, "rsk")).toBe(expected);
    const cdx = await convert(CAFFEINE, "smiles", "cdx");
    expect(cdx).toBeInstanceOf(Uint8Array);
    expect(await smiles(cdx, "cdx")).toBe(expected);
    const cdxml = await convert(CAFFEINE, "smiles", "cdxml");
    expect(await smiles(cdxml, "auto")).toBe(expected);
  });

  test("the SVG preview is drawn with paths", async () => {
    const svg = await renderSvg(CAFFEINE, "smiles");
    expect(svg.startsWith("<svg")).toBeTrue();
    expect(svg).toContain("<path");
  });

  test("PNG previews and figures have the PNG signature", async () => {
    const preview = await renderPng(CAFFEINE, "smiles", { maxWidth: 400, maxHeight: 300 });
    expect(Array.from(preview.subarray(0, 8))).toEqual(PNG);
    const figure = await convert(CAFFEINE, "smiles", "png");
    expect(Array.from(figure.subarray(0, 8))).toEqual(PNG);
    const pdf = await convert(CAFFEINE, "smiles", "pdf");
    expect(new TextDecoder().decode(pdf.slice(0, 4))).toBe("%PDF");
  });

  test("errors carry the app's message", async () => {
    await expect(convert("C(", "smiles", "mol")).rejects.toThrow();
    await expect(convert(CAFFEINE, "smiles", "xyz" as "mol")).rejects.toThrow(
      "Unknown output format xyz",
    );
    await expect(renderPng(CAFFEINE, "smiles", { maxWidth: 0 })).rejects.toThrow();
  });

  test("addFont takes TrueType and OpenType fonts only", async () => {
    await expect(addFont(new Uint8Array([1, 2, 3]))).rejects.toThrow(
      "Not a TrueType or OpenType font",
    );
    const font = readFileSync(
      new URL("../../../tests/fixtures/font-export-103/static-400.subset.ttf", import.meta.url),
    );
    expect(await addFont(font)).toBe(1);
  });

  test("version", async () => {
    expect(await version()).toMatch(/^\d+\.\d+\.\d+$/);
  });
});

const cli = process.env.RESHIKI_CLI;

function desktop(args: string[]): Uint8Array {
  const result = Bun.spawnSync([cli!, "--cli", ...args], { stderr: "pipe" });
  if (result.exitCode !== 0) throw new Error(result.stderr.toString());
  return result.stdout;
}

/** The largest difference between the numbers of two SVGs whose text is
 * otherwise identical, or Infinity when anything but a number differs. */
function svgDeviation(a: string, b: string): number {
  const number = /-?\d+(\.\d+)?(e-?\d+)?/g;
  if (a.replace(number, "#") !== b.replace(number, "#")) return Infinity;
  const left = a.match(number) ?? [];
  const right = b.match(number) ?? [];
  return Math.max(0, ...left.map((n, i) => Math.abs(Number(n) - Number(right[i]))));
}

describe.skipIf(!cli)("matches the desktop CLI", () => {
  for (const input of STRUCTURES) {
    for (const format of ["smiles", "mol", "inchi", "cdxml"] as const) {
      test(`${format} of ${input}`, async () => {
        const expected = new TextDecoder().decode(
          desktop(["convert", "--smiles", input, "--to", format]),
        );
        expect(await convert(input, "smiles", format)).toBe(expected);
      });
    }

    test(`SVG preview of ${input}`, async () => {
      const expected = new TextDecoder().decode(
        desktop(["render", "--smiles", input, "--to", "svg"]),
      );
      const actual = await renderSvg(input, "smiles");
      // Liberation Sans has Arial's advances but not its outlines, so label
      // ink boxes, and the bond ends clipped to them, move slightly.
      const deviation = svgDeviation(actual, expected);
      if (deviation > 0) console.log(`SVG of ${input}: largest difference ${deviation}`);
      expect(deviation).toBeLessThan(1);
    });
  }
});
