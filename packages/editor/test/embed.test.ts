// embed() with just enough of a page to create its frame: the frame never
// starts, so these check teardown and not the editor itself.
import { expect, test } from "bun:test";

const listeners = new Set<unknown>();
Object.assign(globalThis, {
  location: { href: "https://example.test/" },
  document: { createElement: () => ({ style: {}, remove() {}, contentWindow: {} }) },
  addEventListener: (_: string, listener: unknown) => listeners.add(listener),
  removeEventListener: (_: string, listener: unknown) => listeners.delete(listener),
});
const { embed } = await import("../src/embed.js");
const host = { append() {} } as unknown as HTMLElement;

test("destroy before the frame starts settles waiting calls", async () => {
  const editor = embed(host);
  const saving = editor.save();
  editor.destroy();
  await expect(saving).rejects.toThrow("The editor was removed");
  await expect(editor.ready).rejects.toThrow("The editor was removed");
  await expect(editor.load("C", "smiles")).rejects.toThrow("The editor was removed");
  expect(listeners.size).toBe(0);
});
