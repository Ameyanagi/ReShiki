// embed() with just enough of a page to create its frame: the frame never
// starts, so these check teardown and not the editor itself.
import { expect, test } from "bun:test";

const listeners = new Set<unknown>();
let frame: { src?: string } = {};
Object.assign(globalThis, {
  location: { href: "https://example.test/app/" },
  document: {
    createElement: () =>
      (frame = { style: {}, remove() {}, contentWindow: {} } as { src?: string }),
  },
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

test("a frame that never starts rejects waiting calls", async () => {
  const editor = embed(host, { timeout: 10 });
  const saving = editor.save();
  await expect(saving).rejects.toThrow("The editor frame did not start");
  await expect(editor.ready).rejects.toThrow("The editor frame did not start");
  editor.destroy();
  expect(listeners.size).toBe(0);
});

test("the frame gets the page's fonts as full URLs", () => {
  const fontsOf = () => JSON.parse(new URL(frame.src ?? "").searchParams.get("fonts") ?? "null");
  embed(host, { fonts: ["fonts/a.ttf"], cjkFont: "/jp.otf" }).destroy();
  expect(fontsOf()).toEqual({
    fonts: ["https://example.test/app/fonts/a.ttf"],
    cjkFont: "https://example.test/jp.otf",
  });
  embed(host, { cjkFont: false }).destroy();
  expect(fontsOf()).toEqual({ fonts: [], cjkFont: false });
  embed(host).destroy();
  expect(fontsOf()).toBeNull();
});
