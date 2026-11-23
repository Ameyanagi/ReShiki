// Uses the built module and a real module Worker. Unlike the stand-in suite,
// this checks independent Wasm memory, rendering, font replay and coded errors.
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import * as wasm from "../dist/wasm/reshiki_web.js";
import { createApi } from "../src/api.js";
import "./wasm.ts";

test.skipIf(typeof Worker !== "function")(
  "real worker matches direct figures with added fonts and after restart",
  async () => {
    const workers: Worker[] = [];
    let replies = 0;
    const offThread = createApi({
      load: async () => {},
      wasm,
      createWorker: () => {
        const worker = new Worker(new URL("../src/worker.js", import.meta.url), { type: "module" });
        worker.addEventListener("message", ({ data }) => {
          if (data?.type === "result") replies += 1;
        });
        workers.push(worker);
        return worker;
      },
    });
    const direct = createApi({
      load: async () => {},
      wasm,
      workerOptions: { available: () => false },
    });
    try {
      await offThread.addFont(
        readFileSync(
          new URL("../../../tests/fixtures/font-export-103/static-400.subset.ttf", import.meta.url),
        ),
      );
      const doc = JSON.parse(await direct.convert("NCO", "smiles", "rsk"));
      doc.annotations.push({
        id: 1000000,
        position: { x: 0, y: 100 },
        text: "HNO",
        format: { style: { family: "ReShiki Font Export Fixture" } },
      });
      const drawing = JSON.stringify(doc);
      // Multiple PDF Form resources have nondeterministic dictionary/object
      // ordering. Keep exact bytes on a single-form fixture, while all other
      // formats above/below exercise the added font and molecular artwork.
      const pdfDrawing = JSON.stringify({
        version: doc.version,
        atoms: [],
        bonds: [],
        graphics: [
          {
            id: 1,
            kind: "rectangle",
            origin: { x: 0, y: 0 },
            axis_x: { x: 84, y: 0 },
            axis_y: { x: 0, y: 42 },
          },
        ],
      });
      for (const svgText of ["outline", "live"]) {
        expect(await offThread.renderSvg(drawing, "rsk", { svgText })).toBe(
          await direct.renderSvg(drawing, "rsk", { svgText }),
        );
      }
      expect(await offThread.renderPng(drawing, "rsk", { maxWidth: 320, maxHeight: 240 })).toEqual(
        await direct.renderPng(drawing, "rsk", { maxWidth: 320, maxHeight: 240 }),
      );
      for (const format of ["svg", "png", "pdf", "cdxml", "cdx"]) {
        const fixture = format === "pdf" ? pdfDrawing : drawing;
        const expected = await direct.convert(fixture, "rsk", format);
        if (format === "pdf") {
          expect(new TextDecoder().decode(expected).match(/\/Subtype \/Form\b/g)).toHaveLength(1);
        }
        expect(await offThread.convert(fixture, "rsk", format)).toEqual(expected);
      }
      await expect(offThread.renderSvg("CC", "bad")).rejects.toMatchObject({
        code: "invalid_arguments",
      });
      // An idle transport failure discards its font cursor and module instance.
      workers[0].dispatchEvent(new ErrorEvent("error", { message: "restart probe" }));
      expect(await offThread.renderSvg(drawing, "rsk")).toBe(
        await direct.renderSvg(drawing, "rsk"),
      );
      expect(workers).toHaveLength(2);
      // Startup fallback would otherwise hide a broken real worker in this test.
      expect(replies).toBe(9);
    } finally {
      workers.forEach((worker) => worker.terminate());
    }
  },
  60_000,
);
