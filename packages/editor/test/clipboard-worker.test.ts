import { expect, test } from "bun:test";
import { serveWorker } from "../src/worker-handler.js";

test("clipboard operation errors remain byte replies without reloading or retrying", async () => {
  const posted: unknown[][] = [];
  const scope = {
    onmessage: undefined as any,
    postMessage: (...args: unknown[]) => posted.push(args),
  };
  let loads = 0;
  const calls: string[] = [];
  const failure = new TextEncoder().encode(
    JSON.stringify({ version: 1, result: { Err: "PNG unavailable; no usable picture" } }),
  );
  const success = new Uint8Array([1, 2, 3]);
  await serveWorker(scope, {
    load: async () => {
      loads += 1;
    },
    run: async (job: string, request: Uint8Array) => {
      calls.push(job);
      expect(request).toEqual(new Uint8Array([9]));
      return calls.length === 1 ? failure : success;
    },
  });
  await scope.onmessage({ data: ["clipboard", new Uint8Array([9])] });
  expect(posted[1]).toEqual([failure, [failure.buffer]]);
  await scope.onmessage({ data: ["clipboard", new Uint8Array([9])] });
  expect(posted[2]).toEqual([success, [success.buffer]]);
  expect(calls).toEqual(["clipboard", "clipboard"]);
  expect(loads).toBe(1);
});
