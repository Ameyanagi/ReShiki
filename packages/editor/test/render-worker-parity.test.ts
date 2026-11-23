// Real isolated worker memory is required here: installing a font in the
// reference module must not make it available in the editor worker by accident.
import { expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";

const workerGlue = new URL("../dist/worker/reshiki_web_worker.js", import.meta.url);
const workerWasm = new URL("../dist/worker/reshiki_web_worker_bg.wasm", import.meta.url);
const referenceGlue = new URL("../../headless/dist/wasm/reshiki_web.js", import.meta.url);
const referenceWasm = new URL("../../headless/dist/wasm/reshiki_web_bg.wasm", import.meta.url);
const built = [workerGlue, workerWasm, referenceGlue, referenceWasm].every(existsSync);
const encoder = new TextEncoder();
const decoder = new TextDecoder();

function request(document: unknown, format: string, fonts: Uint8Array[]): Uint8Array {
  const header = encoder.encode(
    JSON.stringify(fonts.map((font) => Buffer.from(font).toString("base64"))),
  );
  const body = encoder.encode(JSON.stringify({ operation: "save", document, format }));
  const bytes = new Uint8Array(4 + header.length + body.length);
  new DataView(bytes.buffer).setUint32(0, header.length, true);
  bytes.set(header, 4);
  bytes.set(body, 4 + header.length);
  return bytes;
}

function reply(bytes: Uint8Array): Uint8Array {
  expect(bytes.length).toBeGreaterThanOrEqual(4);
  const length = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint32(0, true);
  expect(length).toBeLessThanOrEqual(bytes.length - 4);
  const header = JSON.parse(decoder.decode(bytes.subarray(4, 4 + length)));
  expect(header).toEqual({ error: null, details: [] });
  return bytes.slice(4 + length);
}

// No application transport or fallback is involved: every result below must
// arrive as bytes posted by the actual editor module Worker.
function editorWorker() {
  const worker = new Worker(new URL("../src/worker.js", import.meta.url), { type: "module" });
  let pending: { resolve(value: Uint8Array): void; reject(error: Error): void } | undefined;
  let failure: Error | undefined;
  let responses = 0;
  let loaded: () => void;
  let failed: (error: Error) => void;
  const ready = new Promise<void>((resolve, reject) => {
    loaded = resolve;
    failed = reject;
  });
  const startup = setTimeout(() => {
    failure = new Error("Editor render worker did not load in time");
    failed(failure);
    worker.terminate();
  }, 60_000);
  worker.onmessage = ({ data }) => {
    if (data === null) {
      clearTimeout(startup);
      loaded();
      return;
    }
    const waiting = pending;
    pending = undefined;
    if (!waiting) {
      failure = new Error("Unexpected editor worker reply");
      return;
    }
    if (data instanceof Uint8Array) {
      responses += 1;
      waiting.resolve(data);
    } else waiting.reject(new Error(String(data)));
  };
  const broken = (event: Event) => {
    event.preventDefault?.();
    clearTimeout(startup);
    failure = new Error(
      "message" in event ? String(event.message) : "Editor worker message failed",
    );
    failed(failure);
    pending?.reject(failure);
    pending = undefined;
  };
  worker.onerror = worker.onmessageerror = broken;
  return {
    get responses() {
      return responses;
    },
    async save(document: unknown, format: string, fonts: Uint8Array[]) {
      await ready;
      if (failure) throw failure;
      if (pending) throw new Error("The parity probe sends one request at a time");
      return new Promise<Uint8Array>((resolve, reject) => {
        const timer = setTimeout(() => {
          pending = undefined;
          failure = new Error("Editor render request did not finish in time");
          worker.terminate();
          reject(failure);
        }, 60_000);
        pending = {
          resolve(bytes) {
            clearTimeout(timer);
            try {
              resolve(reply(bytes));
            } catch (error) {
              reject(error);
            }
          },
          reject(error) {
            clearTimeout(timer);
            reject(error);
          },
        };
        try {
          worker.postMessage(["render", request(document, format, fonts)]);
        } catch (error) {
          pending.reject(error as Error);
          pending = undefined;
        }
      });
    },
    stop() {
      clearTimeout(startup);
      worker.terminate();
    },
  };
}

test.skipIf(!built || typeof Worker !== "function")(
  "editor render worker has isolated fonts, appends snapshots, and replays them after restart",
  async () => {
    // The direct reference loads the same normal exporters in separate memory
    // from the editor worker. Import only after the missing-build skip guard.
    const reference = await import(referenceGlue.href);
    await reference.default({ module_or_path: readFileSync(referenceWasm) });
    const document = JSON.parse(
      decoder.decode(await reference.convert(encoder.encode("NO"), "smiles", "rsk", undefined)),
    );
    document.annotations = [
      ...(document.annotations ?? []),
      {
        id: 1000000,
        position: { x: 0, y: 100 },
        text: "HNO",
        format: { style: { family: "ReShiki Font Export Fixture", size_pt: 14 } },
      },
    ];
    const font = (file: string) =>
      new Uint8Array(
        readFileSync(new URL(`../../../tests/fixtures/font-export-103/${file}`, import.meta.url)),
      );
    const regular = font("static-400.subset.ttf");
    const bold = font("static-700.subset.ttf");
    const direct = (drawing: unknown, format: string) =>
      reference.convert(
        encoder.encode(JSON.stringify(drawing)),
        "rsk",
        format,
        undefined,
      ) as Promise<Uint8Array>;
    let peer = editorWorker();
    try {
      const fallback = await peer.save(document, "svg", []);
      expect(reference.addFont(regular)).toBe(1);
      const expectedRegular = await direct(document, "svg");
      const installed = await peer.save(document, "svg", [regular]);
      expect(installed).not.toEqual(fallback);
      expect(installed).toEqual(expectedRegular);
      expect(await peer.save(document, "svg", [regular])).toEqual(expectedRegular);

      const boldDocument = structuredClone(document);
      boldDocument.annotations.at(-1).format.style.bold = true;
      const beforeAppend = await peer.save(boldDocument, "svg", [regular]);
      expect(reference.addFont(bold)).toBe(1);
      const expectedBold = await direct(boldDocument, "svg");
      const appended = await peer.save(boldDocument, "svg", [regular, bold]);
      expect(appended).not.toEqual(beforeAppend);
      expect(appended).toEqual(expectedBold);
      let expectedPng: Uint8Array | undefined;
      for (const format of ["png", "pdf", "cdxml", "cdx"]) {
        const expected = await direct(boldDocument, format);
        expect(await peer.save(boldDocument, format, [regular, bold])).toEqual(expected);
        if (format === "png") expectedPng = expected;
      }
      expect(await peer.save(boldDocument, "svg", [regular, bold])).toEqual(expectedBold);
      expect(peer.responses).toBe(10);
      await expect(peer.save(document, "svg", [regular])).rejects.toThrow(
        "font snapshot is older than the worker",
      );

      peer.stop();
      peer = editorWorker();
      expect(await peer.save(boldDocument, "svg", [regular, bold])).toEqual(expectedBold);
      expect(await peer.save(boldDocument, "png", [regular, bold])).toEqual(expectedPng);
      expect(peer.responses).toBe(2);
    } finally {
      peer.stop();
    }
  },
  180_000,
);
