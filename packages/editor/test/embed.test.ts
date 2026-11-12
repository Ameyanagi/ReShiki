// embed() with just enough of a page to create its frame and exchange
// messages, without starting the editor itself.
import { expect, test } from "bun:test";

interface FrameCall {
  id: number;
  method: string;
  args: unknown[];
}
interface FrameEvent {
  source: unknown;
  origin: string;
  data: unknown;
}
const listeners = new Set<(event: FrameEvent) => void>();
const makeFrame = () => ({
  src: "",
  style: {},
  remove() {},
  contentWindow: { postMessage(_message: FrameCall, _origin: string) {} },
});
let frame = makeFrame();
Object.assign(globalThis, {
  location: { href: "https://example.test/app/" },
  document: {
    createElement: () => (frame = makeFrame()),
  },
  addEventListener: (_: string, listener: (event: FrameEvent) => void) => listeners.add(listener),
  removeEventListener: (_: string, listener: (event: FrameEvent) => void) =>
    listeners.delete(listener),
});
const { embed } = await import("../src/embed.js");
const host = { append() {} } as unknown as HTMLElement;
const receive = (data: unknown) => {
  const event = {
    source: frame.contentWindow,
    origin: new URL(frame.src).origin,
    data,
  };
  listeners.forEach((listener) => listener(event));
};

test("load forwards import warnings from the frame", async () => {
  const editor = embed(host);
  const result = { warnings: ["First import warning", "Second import warning"] };
  frame.contentWindow.postMessage = (message, origin) => {
    expect(origin).toBe(new URL(frame.src).origin);
    expect(message.method).toBe("load");
    expect(message.args).toEqual(["CCO", "smiles"]);
    receive({ id: message.id, result });
  };
  receive({ event: "ready" });
  try {
    expect(await editor.load("CCO", "smiles")).toEqual(result);
  } finally {
    editor.destroy();
  }
  expect(listeners.size).toBe(0);
});

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
