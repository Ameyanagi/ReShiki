import { expect, test } from "bun:test";
import { serveWorker } from "../src/worker-handler.js";

function scope() {
  const posted: unknown[][] = [];
  return {
    posted,
    onmessage: undefined as any,
    postMessage: (...args: unknown[]) => posted.push(args),
  };
}

test("the worker announces readiness before serial asynchronous jobs and transfers only replies", async () => {
  const worker = scope();
  let release!: () => void;
  const first = new Promise<void>((resolve) => {
    release = resolve;
  });
  const calls: string[] = [];
  const reply = new Uint8Array([7, 8]);
  await serveWorker(worker, {
    load: async () => {},
    run: async (job: string, request: Uint8Array) => {
      calls.push(job);
      if (job === "render") await first;
      expect(request).toEqual(new Uint8Array([1, 2]));
      return reply;
    },
  });
  expect(worker.posted).toEqual([[null]]);
  const request = new Uint8Array([1, 2]);
  const a = worker.onmessage({ data: ["render", request] });
  const b = worker.onmessage({ data: ["naming", request] });
  await Promise.resolve();
  expect(calls).toEqual(["render"]);
  release();
  await Promise.all([a, b]);
  expect(calls).toEqual(["render", "naming"]);
  expect(worker.posted.slice(1)).toEqual([
    [reply, [reply.buffer]],
    [reply, [reply.buffer]],
  ]);
  expect(request.byteLength).toBe(2);
});

test("initialization failure is reported without running queued work", async () => {
  const worker = scope();
  let ran = false;
  await serveWorker(worker, {
    load: async () => {
      throw new Error("module unavailable");
    },
    run: () => {
      ran = true;
    },
  });
  await worker.onmessage({ data: ["render", new Uint8Array()] });
  expect(worker.posted).toEqual([["module unavailable"]]);
  expect(ran).toBeFalse();
});

test("a real operation failure is reported and does not poison the next request", async () => {
  const worker = scope();
  await serveWorker(worker, {
    load: async () => {},
    run: async (job: string) => {
      if (job === "bad") throw new Error("invalid drawing");
      return new Uint8Array([3]);
    },
  });
  await worker.onmessage({ data: ["bad", new Uint8Array()] });
  await worker.onmessage({ data: ["render", new Uint8Array()] });
  expect(worker.posted[1]).toEqual(["invalid drawing"]);
  expect(worker.posted[2][0]).toEqual(new Uint8Array([3]));
});
