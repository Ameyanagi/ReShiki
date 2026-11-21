import { describe, expect, test } from "bun:test";
import { convert, predictNmr } from "../src/index.js";
import type { NmrNucleus } from "../src/index.js";
import "./wasm.ts";

describe("NMR prediction", () => {
  test("bytes and drawings retain the same prediction", async () => {
    const smiles = "CCOC(C)=O";
    const expected = await predictNmr(smiles, "smiles", "13C");
    const bytes = new TextEncoder().encode(smiles);
    expect(await predictNmr(bytes, "smiles", "13C")).toEqual(expected);
    const drawing = await convert(smiles, "smiles", "rsk");
    expect(await predictNmr(drawing, "rsk", "13C")).toEqual(expected);
    expect(await predictNmr(drawing, "reshiki", "13C")).toEqual(expected);
    const cdx = await convert(smiles, "smiles", "cdx");
    expect(await predictNmr(cdx, "cdx", "13C")).toEqual(expected);
  });

  test("missing reference data keeps null values and a note", async () => {
    const protons = await predictNmr("CCO", "smiles", "1H");
    const hydroxyl = protons.rows.find((row) => row.atom === "3")!;
    expect(hydroxyl).toMatchObject({
      ppm: null,
      shell: null,
      references: null,
      sd: null,
      min: null,
      max: null,
      hydrogen_count: 1,
      note: expect.stringContaining("Exchangeable"),
    });
    const carbons = await predictNmr("CCO", "smiles", "13C");
    expect(carbons.rows).toHaveLength(2);
    expect(carbons.rows.every((row) => row.ppm === null && row.hydrogen_count === null)).toBeTrue();
  });

  test("native atom IDs stay decimal strings above the JS safe integer limit", async () => {
    const drawing = await convert("C", "smiles", "rsk");
    const id = "9007199254740993";
    const wide = drawing.replace(/("id"\s*:\s*)1(?=\s*[,}])/, (_, prefix) => `${prefix}${id}`);
    expect(wide).not.toBe(drawing);
    const prediction = await predictNmr(wide, "rsk", "13C");
    expect(prediction.rows[0].atom).toBe(id);
  });

  test("drawn H IDs are strings on their heavy atom row", async () => {
    const drawing = JSON.parse(await convert("C", "smiles", "rsk"));
    const carbon = drawing.atoms[0].id;
    drawing.atoms.push({ id: 2, element: "H", position: { x: -30, y: 0 } });
    drawing.bonds.push({ id: 3, a: carbon, b: 2, order: 1, display: "plain" });
    const prediction = await predictNmr(JSON.stringify(drawing), "rsk", "1H");
    expect(prediction.rows).toHaveLength(1);
    expect(prediction.rows[0]).toMatchObject({
      atom: String(carbon),
      hydrogen_count: 4,
      hydrogens: ["2"],
    });
  });

  for (const nucleus of ["H1", "13c", "15N", "", undefined, null, 1]) {
    test(`invalid nucleus ${JSON.stringify(nucleus)} has the argument error code`, async () => {
      await expect(predictNmr("C(", "smiles", nucleus as NmrNucleus)).rejects.toMatchObject({
        code: "invalid_arguments",
        message: "nucleus must be 1H or 13C",
      });
    });
  }

  for (const [smiles, nucleus, message] of [
    ["CCO.CC", "13C", "NMR prediction needs exactly one molecule; this drawing has 2"],
    ["O", "13C", "This molecule has no supported ¹³C sites"],
    ["ClC(Cl)(Cl)Cl", "1H", "This molecule has no supported ¹H sites"],
    ["[Fe]", "13C", undefined],
    ["[CH3-]", "13C", undefined],
    ["[2H]C", "1H", undefined],
    ["C(", "1H", undefined],
  ] as const) {
    test(`${nucleus} prediction of ${smiles} has the operation error code`, async () => {
      const expected = message === undefined ? { code: "failed" } : { code: "failed", message };
      await expect(predictNmr(smiles, "smiles", nucleus)).rejects.toMatchObject(expected);
    });
  }

  test("an unknown input format has the argument error code", async () => {
    await expect(predictNmr("CCO", "xyz" as "mol", "1H")).rejects.toMatchObject({
      code: "invalid_arguments",
      message: "Unknown input format xyz",
    });
  });
});
