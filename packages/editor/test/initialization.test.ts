import { expect, test } from "bun:test";
import { createInitializer } from "../src/initialization.js";

test("only the default editor module enables the bundled render worker", async () => {
  for (const module of [undefined, "custom.wasm", new Uint8Array([0]), new Response("wasm")]) {
    const loaded: unknown[] = [];
    const configured: unknown[][] = [];
    const factory = () => {};
    const init = createInitializer({
      load: async (input: unknown) => {
        loaded.push(input);
      },
      configureWorker: (...args: unknown[]) => configured.push(args),
      worker: factory,
      available: () => true,
    });
    await init(module);
    await init();
    expect(loaded).toEqual([module === undefined ? undefined : { module_or_path: module }]);
    expect(configured).toEqual([[factory, module === undefined]]);
  }
});

test("failed initialization retries with the next source, while no Worker retains local export", async () => {
  let attempts = 0;
  const configured: boolean[] = [];
  const init = createInitializer({
    load: async () => {
      if (++attempts === 1) throw new Error("module unavailable");
    },
    configureWorker: (_factory: unknown, render: boolean) => configured.push(render),
    worker: () => {},
    available: () => true,
  });
  await expect(init("custom.wasm")).rejects.toThrow("module unavailable");
  await init();
  expect(configured).toEqual([true]);
  const local = createInitializer({
    load: async () => {},
    configureWorker: () => {
      throw new Error("Worker must not be configured");
    },
    worker: () => {},
    available: () => false,
  });
  await local("custom.wasm");
});

test("worker configuration retry cannot promote a loaded custom module to the bundled renderer", async () => {
  const configured: boolean[] = [];
  const init = createInitializer({
    load: async () => {},
    configureWorker: (_factory: unknown, render: boolean) => {
      configured.push(render);
      if (configured.length === 1) throw new Error("worker configuration failed");
    },
    worker: () => {},
    available: () => true,
  });
  await expect(init("custom.wasm")).rejects.toThrow("worker configuration failed");
  await init();
  expect(configured).toEqual([false, false]);
});
