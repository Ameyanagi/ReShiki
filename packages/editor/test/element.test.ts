// The editor's loader and <reshiki-editor> with a stand-in for the editor
// wasm, which these tests never start, and just enough of a page.
import { expect, mock, test } from "bun:test";

let loads = 0;
let mounts = 0;
let freed = 0;
let dirty = false;
const warnings = ["First import warning", "Second import warning"];
const imported: { data: Uint8Array; format: string }[] = [];
const started: { data?: Uint8Array; format: string }[] = [];
mock.module("../dist/editor/reshiki.js", () => ({
  default: async () => {
    loads += 1;
    if (loads === 1) throw new Error("network error");
  },
  mount: async (_element: unknown, data: Uint8Array | undefined, format: string) => {
    mounts += 1;
    started.push({ data, format });
  },
  load: async (data: Uint8Array, format: string) => {
    imported.push({ data, format });
    return {
      warnings,
      free() {
        freed += 1;
      },
    };
  },
  onChange() {},
  isDirty: () => dirty,
  setWorker() {},
  onDirtyChange() {},
  onTitleChange() {},
}));

class Element extends EventTarget {
  isConnected = true;
  style: Record<string, string> = {};
  attributes: Record<string, string> = {};
  getAttribute(name: string) {
    return this.attributes[name] ?? null;
  }
}
Object.assign(globalThis, {
  HTMLElement: Element,
  customElements: { get() {}, define() {} },
});
const { initEditor } = await import("../src/editor.js");
const { ReshikiEditor } = await import("../src/element.js");

test("a failed editor load is retried by the next call", async () => {
  await expect(initEditor()).rejects.toThrow("network error");
  await initEditor();
  expect(loads).toBe(2);
});

test("a src that answers with an HTTP error fires error and does not mount", async () => {
  const fetched = globalThis.fetch;
  globalThis.fetch = (async () =>
    new Response("<h1>Not found</h1>", { status: 404 })) as unknown as typeof fetch;
  try {
    const element = new ReshikiEditor();
    (element as unknown as Element).attributes.src = "missing.mol";
    const fired = new Promise<ErrorEvent>((resolve) =>
      element.addEventListener("error", (event) => resolve(event as ErrorEvent)),
    );
    element.connectedCallback();
    const event = await fired;
    expect(event.message).toBe("Could not load missing.mol: HTTP 404");
    await expect(element.ready).rejects.toThrow("HTTP 404");
    expect(mounts).toBe(0);
  } finally {
    globalThis.fetch = fetched;
  }
});

test("the element forwards load's copied import warnings", async () => {
  const element = new ReshikiEditor();
  element.connectedCallback();
  const result = await element.load("CCO", "smiles");
  expect(result).toEqual({ warnings });
  expect(result.warnings).not.toBe(warnings);
  expect(Array.isArray(result.warnings)).toBeTrue();
  expect(imported).toEqual([{ data: new TextEncoder().encode("CCO"), format: "smiles" }]);
  expect(freed).toBe(1);
  result.warnings.push("Only in the caller's array");
  expect(warnings).toEqual(["First import warning", "Second import warning"]);
});

// Changes `src` (and `format`) the way the browser reports it, then waits for
// the load to settle. Answers the `beforeload` details the element fired.
async function changeSrc(
  element: InstanceType<typeof ReshikiEditor>,
  src: string,
  format?: string,
) {
  const attributes = (element as unknown as Element).attributes;
  const asked: unknown[] = [];
  const listen = (event: Event) => asked.push((event as CustomEvent).detail);
  element.addEventListener("beforeload", listen);
  const before = attributes.src ?? null;
  attributes.src = src;
  element.attributeChangedCallback("src", before, src);
  if (format) {
    attributes.format = format;
    element.attributeChangedCallback("format", null, format);
  }
  await new Promise((resolve) => setTimeout(resolve));
  element.removeEventListener("beforeload", listen);
  return asked;
}

test("a changed src loads the new drawing, unless the page prevents it", async () => {
  const fetched = globalThis.fetch;
  globalThis.fetch = (async (src: string) =>
    src === "missing.mol"
      ? new Response("", { status: 404 })
      : new Response(src)) as unknown as typeof fetch;
  const text = (data: Uint8Array) => new TextDecoder().decode(data);
  try {
    const element = new ReshikiEditor();
    // Attributes the element starts with are reported before it connects.
    element.attributeChangedCallback("src", null, "first.mol");
    element.connectedCallback();
    await element.ready;
    imported.length = 0;

    // Clean: the new drawing loads, with src and format changed together.
    expect(await changeSrc(element, "clean.mol", "mol")).toEqual([
      { src: "clean.mol", format: "mol", dirty: false },
    ]);
    expect(imported.map(({ data, format }) => [text(data), format])).toEqual([
      ["clean.mol", "mol"],
    ]);

    // Unsaved changes: the page is told, and the drawing loads as asked.
    dirty = true;
    expect(await changeSrc(element, "dirty.mol")).toEqual([
      { src: "dirty.mol", format: "mol", dirty: true },
    ]);
    expect(imported.map(({ data }) => text(data))).toEqual(["clean.mol", "dirty.mol"]);

    // Prevented, with and without unsaved changes: the drawing in front stays.
    const prevent = (event: Event) => event.preventDefault();
    element.addEventListener("beforeload", prevent);
    for (dirty of [true, false]) {
      expect(await changeSrc(element, `kept-${dirty}.mol`)).toEqual([
        { src: `kept-${dirty}.mol`, format: "mol", dirty },
      ]);
    }
    element.removeEventListener("beforeload", prevent);
    expect(imported).toHaveLength(2);

    // A src that cannot be loaded fires error and keeps the drawing.
    const errors: string[] = [];
    element.addEventListener("error", (event) => errors.push((event as ErrorEvent).message));
    await changeSrc(element, "missing.mol");
    expect(errors).toEqual(["Could not load missing.mol: HTTP 404"]);
    expect(imported).toHaveLength(2);
  } finally {
    dirty = false;
    globalThis.fetch = fetched;
  }
});

test("an earlier src never loads after a later change, also a prevented or removed one", async () => {
  const fetched = globalThis.fetch;
  const answers: Record<string, (response: Response) => void> = {};
  globalThis.fetch = ((src: string) =>
    new Promise<Response>((resolve) => (answers[src] = resolve))) as unknown as typeof fetch;
  const settle = () => new Promise((resolve) => setTimeout(resolve));
  try {
    const element = new ReshikiEditor();
    const attributes = (element as unknown as Element).attributes;
    element.connectedCallback();
    await element.ready;
    imported.length = 0;
    const errors: string[] = [];
    element.addEventListener("error", (event) => errors.push((event as ErrorEvent).message));
    const loaded = () => imported.map(({ data }) => new TextDecoder().decode(data));

    // The later change answers first; the earlier one neither loads nor reports its error.
    await changeSrc(element, "slow.mol");
    await changeSrc(element, "fast.mol");
    answers["fast.mol"](new Response("fast"));
    answers["slow.mol"](new Response("", { status: 404 }));
    await settle();
    expect(loaded()).toEqual(["fast"]);
    expect(errors).toEqual([]);

    // The page is asked when the file is here, so edits made meanwhile count.
    const asked: unknown[] = [];
    const prevent = (event: Event) => {
      asked.push((event as CustomEvent).detail);
      event.preventDefault();
    };
    element.addEventListener("beforeload", prevent);
    await changeSrc(element, "edited-meanwhile.mol");
    expect(asked).toEqual([]);
    dirty = true;
    answers["edited-meanwhile.mol"](new Response("lost edits"));
    await settle();
    expect(asked).toEqual([{ src: "edited-meanwhile.mol", format: "auto", dirty: true }]);
    element.removeEventListener("beforeload", prevent);

    // Preventing a later load does not let an earlier one through.
    element.addEventListener("beforeload", prevent);
    await changeSrc(element, "earlier.mol");
    await changeSrc(element, "later.mol");
    answers["later.mol"](new Response("later"));
    await settle();
    answers["earlier.mol"](new Response("earlier"));
    await settle();
    element.removeEventListener("beforeload", prevent);
    expect(loaded()).toEqual(["fast"]);

    // A listener that removes src keeps the drawing, as removing it always does.
    const remove = () => {
      delete attributes.src;
      element.attributeChangedCallback("src", "removed.mol", null);
    };
    element.addEventListener("beforeload", remove);
    await changeSrc(element, "removed.mol");
    answers["removed.mol"](new Response("removed"));
    await settle();
    element.removeEventListener("beforeload", remove);
    expect(loaded()).toEqual(["fast"]);

    // Removing src drops a load that is still on its way.
    await changeSrc(element, "pending.mol");
    delete attributes.src;
    element.attributeChangedCallback("src", "pending.mol", null);
    answers["pending.mol"](new Response("pending"));
    await settle();
    expect(loaded()).toEqual(["fast"]);
  } finally {
    dirty = false;
    globalThis.fetch = fetched;
  }
});

test("a src changed while the first file is on its way replaces it at the start", async () => {
  const fetched = globalThis.fetch;
  const answers: Record<string, (response: Response) => void> = {};
  const stopped: string[] = [];
  globalThis.fetch = ((src: string, { signal }: { signal: AbortSignal }) =>
    new Promise<Response>((resolve, reject) => {
      answers[src] = resolve;
      signal.addEventListener("abort", () => {
        stopped.push(src);
        reject(signal.reason);
      });
    })) as unknown as typeof fetch;
  const settle = () => new Promise((resolve) => setTimeout(resolve));
  const text = ({ data, format }: { data?: Uint8Array; format: string }) => [
    new TextDecoder().decode(data),
    format,
  ];
  try {
    const element = new ReshikiEditor();
    const attributes = (element as unknown as Element).attributes;
    const errors: string[] = [];
    element.addEventListener("error", (event) => errors.push((event as ErrorEvent).message));
    Object.assign(attributes, { src: "a.mol", format: "mol" });
    element.connectedCallback();
    // The page changes both while the first file is still on its way; that
    // file never arrives.
    Object.assign(attributes, { src: "b.cdx", format: "cdx" });
    element.attributeChangedCallback("src", "a.mol", "b.cdx");
    element.attributeChangedCallback("format", "mol", "cdx");
    started.length = 0;
    imported.length = 0;
    await settle();
    expect(stopped).toEqual(["a.mol"]);
    answers["b.cdx"](new Response("b"));
    await element.ready;
    await settle();
    // The editor starts with the new pair, and does not load it a second time.
    expect(started.map(text)).toEqual([["b", "cdx"]]);
    expect(imported).toEqual([]);
    expect(errors).toEqual([]);
  } finally {
    globalThis.fetch = fetched;
  }
});

test("a changed src starts an editor whose first src could not be loaded", async () => {
  const fetched = globalThis.fetch;
  globalThis.fetch = (async (src: string) =>
    src === "typo.mol"
      ? new Response("", { status: 404 })
      : new Response(src)) as unknown as typeof fetch;
  try {
    const element = new ReshikiEditor();
    const errors: string[] = [];
    element.addEventListener("error", (event) => errors.push((event as ErrorEvent).message));
    (element as unknown as Element).attributes.src = "typo.mol";
    element.connectedCallback();
    await expect(element.ready).rejects.toThrow("HTTP 404");
    started.length = 0;
    await changeSrc(element, "right.mol");
    await element.ready;
    expect(errors).toEqual(["Could not load typo.mol: HTTP 404"]);
    expect(started.map(({ data }) => new TextDecoder().decode(data))).toEqual(["right.mol"]);
  } finally {
    globalThis.fetch = fetched;
  }
});
