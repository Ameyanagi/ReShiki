// One input, the same answers on every surface. tests/fixtures/agent-api/
// parity.json is written by `reshiki --cli` (scripts/agent_parity_fixture.py)
// and also checked by tests/cli.rs and crates/python/tests/test_parity.py.
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { convert, init, inchi, renderSvg, smiles } from "../src/index.js";

const fixture = JSON.parse(
  readFileSync(new URL("../../../tests/fixtures/agent-api/parity.json", import.meta.url), "utf8"),
);
const molecule = fixture.molecule.smiles as string;

await init(readFileSync(new URL("../dist/wasm/reshiki_web_bg.wasm", import.meta.url)));

describe("cross-surface parity", () => {
  test("the molecule converts as the command line converts it", async () => {
    expect(await convert(molecule, "smiles", "smiles")).toBe(fixture.molecule.outputs.smiles);
    expect(await smiles(molecule, "smiles")).toBe(fixture.molecule.outputs.smiles);
    expect(await convert(molecule, "smiles", "inchi")).toBe(fixture.molecule.outputs.inchi);
    expect(await inchi(molecule, "smiles")).toBe(fixture.molecule.outputs.inchi);
  });

  test("the reaction converts as the command line converts it", async () => {
    expect(await convert(fixture.reaction.rsmi, "rsmi", "rsmi")).toBe(
      fixture.reaction.outputs.rsmi,
    );
    expect(await convert(fixture.reaction.rsmi, "rsmi", "rxn")).toStartWith("$RXN");
  });

  test("rsk and reshiki name the same format", async () => {
    const drawing = await convert(molecule, "smiles", "rsk");
    expect(await convert(molecule, "smiles", "reshiki")).toBe(drawing);
    expect(await smiles(drawing, "reshiki")).toBe(fixture.molecule.outputs.smiles);
  });

  test("SVG text is outlined unless svgText is live", async () => {
    for (const svg of [
      await convert(molecule, "smiles", "svg"),
      await convert(molecule, "smiles", "svg", { svgText: "outline" }),
      await renderSvg(molecule, "smiles"),
    ]) {
      expect(svg).toContain("<path");
      expect(svg).not.toContain("<text");
    }
    expect(await convert(molecule, "smiles", "svg", { svgText: "live" })).toContain("<text");
    expect(await renderSvg(molecule, "smiles", { svgText: "live" })).toContain("<text");
  });

  test("errors carry the operation error code", async () => {
    const code = (call: Promise<unknown>) =>
      call.then(
        () => "no error",
        (error: { code?: string }) => error.code,
      );
    expect(await code(convert("C(", "smiles", "mol"))).toBe("failed");
    expect(await code(convert(molecule, "smiles", "xyz" as "mol"))).toBe("invalid_arguments");
    expect(await code(convert(molecule, "xyz" as "mol", "mol"))).toBe("invalid_arguments");
    expect(await code(convert(molecule, "smiles", "svg", { svgText: "bold" as "live" }))).toBe(
      "invalid_arguments",
    );
  });
});
