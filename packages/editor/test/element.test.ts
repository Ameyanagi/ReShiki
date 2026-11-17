// The editor's loader and <reshiki-editor> with a stand-in for the editor
// wasm, which these tests never start, and just enough of a page.
import { expect, mock, test } from "bun:test";

let loads = 0;
let mounts = 0;
let freed = 0;
let dirty = false;
const warnings = ["First import warning", "Second import warning"];
const imported: { data: Uint8Array; format: string }[] = [];
mock.module("../dist/editor/reshiki.js", () => ({
  default: async () => {
    loads += 1;
    if (loads === 1) throw new Error("network error");
  },
  mount: async () => {
    mounts += 1;
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

test("only the latest of two src changes loads", async () => {
  const fetched = globalThis.fetch;
  const answers: Record<string, (response: Response) => void> = {};
  globalThis.fetch = ((src: string) =>
    new Promise<Response>((resolve) => (answers[src] = resolve))) as unknown as typeof fetch;
  try {
    const element = new ReshikiEditor();
    element.connectedCallback();
    await element.ready;
    imported.length = 0;
    await changeSrc(element, "slow.mol");
    await changeSrc(element, "fast.mol");
    answers["fast.mol"](new Response("fast"));
    answers["slow.mol"](new Response("slow"));
    await new Promise((resolve) => setTimeout(resolve));
    expect(imported.map(({ data }) => new TextDecoder().decode(data))).toEqual(["fast"]);
  } finally {
    globalThis.fetch = fetched;
  }
});
