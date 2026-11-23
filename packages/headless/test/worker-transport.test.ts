// No generated module is imported: these exercise the public wrapper and real
// transport/handler with isolated stand-ins, even before a Wasm build.
import { expect, test } from "bun:test";
import { createApi } from "../src/api.js";
import { createFigureWorker } from "../src/figure-worker.js";
import { createWorkerHandler } from "../src/worker-handler.js";

const encoder = new TextEncoder();
const decoder = new TextDecoder();
const pause = (ms = 0) => new Promise((resolve) => setTimeout(resolve, ms));
const coded = (message: string, code = "invalid_arguments") =>
  Object.assign(new Error(message), { code });

function setup(options: Record<string, unknown> = {}) {
  const workers: any[] = [];
  const modules: unknown[] = [];
  const direct: unknown[] = [];
  const initialized: unknown[] = [];
  const module = { customCompiledModule: true };
  let operation: ((name: string, args: unknown[], fonts: number[]) => unknown) | undefined;
  const wasmFor = (main = false) => {
    const fonts: number[] = [];
    const invoke = async (name: string, ...args: unknown[]) => {
      if (main) direct.push([name, args]);
      if (!main && operation) return operation(name, args, fonts);
      if (args[1] === "bad") throw coded("Unknown input format bad");
      const result = JSON.stringify({ name, args: [...(args[0] as Uint8Array)], fonts });
      return name === "renderSvg" ? result : encoder.encode(result);
    };
    return {
      workerModule: () => module,
      addFont(bytes: Uint8Array) {
        if (!bytes[0]) throw coded("Not a TrueType or OpenType font", "failed");
        fonts.push(bytes[0]);
        return 1;
      },
      convert: (...args: unknown[]) => invoke("convert", ...args),
      renderSvg: (...args: unknown[]) => invoke("renderSvg", ...args),
      renderPng: (...args: unknown[]) => invoke("renderPng", ...args),
    };
  };
  class FakeWorker {
    onmessage: any;
    onerror: any;
    onmessageerror: any;
    terminated = false;
    messages: any[] = [];
    receive = createWorkerHandler(
      async (input: any) => {
        modules.push(input.module_or_path);
      },
      wasmFor(),
      (data: any) => {
        queueMicrotask(() => {
          if (!this.terminated) this.onmessage?.({ data: structuredClone(data) });
        });
      },
    );
    postMessage(message: unknown) {
      const data = structuredClone(message);
      this.messages.push(data);
      queueMicrotask(() => {
        if (!this.terminated) void this.receive({ data });
      });
    }
    terminate() {
      this.terminated = true;
    }
  }
  const api = createApi({
    load: async (input: unknown) => {
      initialized.push(input);
    },
    wasm: wasmFor(true),
    createWorker: () => {
      const worker = new FakeWorker();
      workers.push(worker);
      return worker;
    },
    workerOptions: { available: () => true, ...options },
  });
  return {
    api,
    workers,
    modules,
    module,
    direct,
    initialized,
    setOperation: (value: typeof operation) => {
      operation = value;
    },
  };
}

test("custom init inputs are passed unchanged; failed loads retry and workers get the compiled module", async () => {
  const module = new WebAssembly.Module(new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0]));
  for (const input of [
    undefined,
    "custom.wasm",
    new URL("https://example.test/custom.wasm"),
    new Request("https://example.test/custom.wasm"),
    new Response(new Uint8Array([1])),
    new Uint8Array([1]),
    new ArrayBuffer(1),
    module,
  ]) {
    const calls: unknown[] = [];
    const api = createApi({
      load: async (value: unknown) => {
        calls.push(value);
        if (calls.length === 1) throw new Error("load failed");
      },
      wasm: {},
      createWorker: () => {
        throw new Error("unused");
      },
    });
    await expect(api.init(input)).rejects.toThrow("load failed");
    await api.init(input);
    await api.init();
    expect(calls).toHaveLength(2);
    expect(input === undefined ? calls[1] : (calls[1] as any).module_or_path).toBe(input);
  }
  const run = setup();
  await run.api.init(new Response("custom bytes"));
  expect(run.workers).toHaveLength(0);
  await run.api.renderSvg("CC", "smiles");
  expect(run.modules).toEqual([run.module]);
  expect(run.initialized).toHaveLength(1);
  expect(run.direct).toHaveLength(0);
});

test("accepted fonts replay in order, caller buffers stay intact, and SVG/binary output types stay unchanged", async () => {
  const run = setup();
  const font = new Uint8Array([1]);
  expect(await run.api.addFont(font)).toBe(1);
  font[0] = 99;
  await expect(run.api.addFont(new Uint8Array([0]))).rejects.toMatchObject({ code: "failed" });
  const data = new Uint8Array([7, 8]);
  const first = await run.api.renderPng(data, "rsk", { maxWidth: 40, maxHeight: 30 });
  expect(first).toBeInstanceOf(Uint8Array);
  expect(JSON.parse(decoder.decode(first))).toMatchObject({ args: [7, 8], fonts: [1] });
  expect([...data]).toEqual([7, 8]);
  expect(data.byteLength).toBe(2);
  await run.api.addFont(new Uint8Array([2]));
  expect(JSON.parse(await run.api.renderSvg(data, "rsk"))).toMatchObject({ fonts: [1, 2] });
  for (const format of ["svg", "pdf", "cdxml", "cdx"]) {
    const output = await run.api.convert(data, "rsk", format);
    expect(typeof output === "string").toBe(format === "svg" || format === "cdxml");
  }
  expect(run.workers).toHaveLength(1);
  expect(
    run.workers[0].messages.filter((m: any) => m.type === "run").map((m: any) => m.fonts.length),
  ).toEqual([1, 1, 0, 0, 0, 0]);
});

test("operation failures retain error.code and reuse the worker without executing directly", async () => {
  const run = setup();
  await expect(run.api.renderPng("CC", "bad")).rejects.toMatchObject({
    message: "Unknown input format bad",
    code: "invalid_arguments",
  });
  await run.api.renderPng("CC", "smiles");
  expect(run.workers).toHaveLength(1);
  expect(run.direct).toHaveLength(0);
});

test("concurrent calls are serialized and queued input is owned", async () => {
  const run = setup();
  let release: (() => void) | undefined;
  const started: number[] = [];
  run.setOperation(async (_name, args) => {
    started.push((args[0] as Uint8Array)[0]);
    if (started.length === 1)
      await new Promise<void>((resolve) => {
        release = resolve;
      });
    return new Uint8Array([started.at(-1)!]);
  });
  await run.api.init();
  const first = run.api.renderPng(new Uint8Array([1]), "rsk");
  await pause();
  const input = new Uint8Array([2]);
  const second = run.api.renderPng(input, "rsk");
  await pause();
  input[0] = 3;
  expect(started).toEqual([1]);
  release!();
  await Promise.all([first, second]);
  expect(started).toEqual([1, 2]);
});

test("no Worker and startup-only failures fall back before any operation is dispatched", async () => {
  const noWorker = setup({ available: () => false });
  await noWorker.api.renderSvg("CC", "smiles");
  expect(noWorker.workers).toHaveLength(0);
  expect(noWorker.direct).toHaveLength(1);
  for (const mode of ["constructor", "init", "timeout"]) {
    let direct = 0;
    let dispatched = 0;
    let terminated = false;
    const client = createFigureWorker({
      module: () => ({}),
      fonts: [],
      available: () => true,
      loadTimeout: 5,
      direct: () => {
        direct += 1;
        return "fallback";
      },
      createWorker: () => {
        if (mode === "constructor") throw new Error("CSP");
        const worker: any = {
          terminate() {
            terminated = true;
          },
          postMessage(data: any) {
            if (data.type === "run") dispatched += 1;
            else if (mode === "init")
              queueMicrotask(() =>
                worker.onmessage({
                  data: { type: "startup-error", error: { message: "init failed" } },
                }),
              );
          },
        };
        return worker;
      },
    });
    expect(await client.run("renderSvg", [new Uint8Array([1])])).toBe("fallback");
    expect(direct).toBe(1);
    expect(dispatched).toBe(0);
    if (mode !== "constructor") expect(terminated).toBeTrue();
  }
});

test("a timed-out dispatched job rejects once, restarts, and replays fonts without a direct retry", async () => {
  const run = setup({ jobTimeout: 5 });
  await run.api.addFont(new Uint8Array([4]));
  run.setOperation(() => new Promise(() => {}));
  await expect(run.api.renderPng("CC", "smiles")).rejects.toMatchObject({
    code: "failed",
    message: "Rendering reached its time limit",
  });
  expect(run.workers[0].terminated).toBeTrue();
  run.setOperation(undefined);
  const result = JSON.parse(await run.api.renderSvg("CC", "smiles"));
  expect(result.fonts).toEqual([4]);
  expect(run.workers).toHaveLength(2);
  expect(run.direct).toHaveLength(0);
});

test("transport errors terminate the active worker and the next request starts another", async () => {
  const run = setup();
  run.setOperation(() => new Promise(() => {}));
  const result = run.api.renderPng("CC", "smiles");
  await pause();
  run.workers[0].onmessageerror({ message: "clone failed" });
  await expect(result).rejects.toMatchObject({ code: "failed", message: "clone failed" });
  expect(run.workers[0].terminated).toBeTrue();
  run.setOperation(undefined);
  await run.api.renderSvg("CC", "smiles");
  expect(run.workers).toHaveLength(2);
  expect(run.direct).toHaveLength(0);
});

test("pending count and queue wait are bounded", async () => {
  const run = setup({ maxPending: 2, queueTimeout: 5, jobTimeout: 1000 });
  let release: (() => void) | undefined;
  run.setOperation(async () => {
    await new Promise<void>((resolve) => {
      release = resolve;
    });
    return new Uint8Array([1]);
  });
  const first = run.api.renderPng("CC", "smiles");
  await pause();
  const second = run.api.renderPng("CC", "smiles");
  await expect(run.api.renderPng("CC", "smiles")).rejects.toThrow("queue is full");
  await expect(second).rejects.toThrow("waited too long");
  release!();
  await first;
});
