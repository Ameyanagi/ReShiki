// The editor's loader and <reshiki-editor> with a stand-in for the editor
// wasm, which these tests never start, and just enough of a page.
import { expect, mock, test } from "bun:test";

let loads = 0;
let mounts = 0;
mock.module("../dist/editor/reshiki.js", () => ({
  default: async () => {
    loads += 1;
    if (loads === 1) throw new Error("network error");
  },
  mount: async () => {
    mounts += 1;
  },
  onChange() {},
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
