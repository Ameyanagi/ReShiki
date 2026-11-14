// The inline script of frame.html, run against a stub editor and just enough
// of a page: it must post only to the page that embeds it and fetch fonts
// only from that page's origin or its own.
import { expect, test } from "bun:test";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

interface Posted {
  message: { event?: string; id?: number; result?: unknown; error?: string };
  origin: string;
}
type Listener = (event: { source: unknown; origin: string; data: unknown }) => unknown;

const html = await readFile(new URL("../src/frame.html", import.meta.url), "utf8");
const script = /<script type="module">([\s\S]*?)<\/script>/.exec(html)?.[1] ?? "";

/** Runs the frame script as a page at `frame`, embedded by a page on `embedder`. */
async function run(frame: string, embedder: string) {
  const folder = await mkdtemp(join(tmpdir(), "reshiki-frame-"));
  await writeFile(
    join(folder, "editor.js"),
    `export const mount = async (_element, options) => {
       globalThis.mounted = options;
       return { save: async (format) => "saved " + format };
     };`,
  );
  await writeFile(join(folder, "frame.js"), script);
  const posted: Posted[] = [];
  const listeners: Listener[] = [];
  const parent = {
    postMessage: (message: Posted["message"], origin: string) => posted.push({ message, origin }),
  };
  Object.assign(globalThis, {
    parent,
    location: new URL(frame),
    document: { referrer: `${embedder}/page.html`, getElementById: () => ({}) },
    addEventListener: (_: string, listener: Listener) => listeners.push(listener),
  });
  await import(pathToFileURL(join(folder, "frame.js")).href);
  await new Promise((resolve) => setTimeout(resolve, 0));
  const mounted = (globalThis as { mounted?: { fonts?: string[]; cjkFont?: unknown } }).mounted;
  return { posted, listeners, parent, mounted };
}

test("the frame posts only to the page that embeds it", async () => {
  const { posted, listeners, parent } = await run(
    "https://frames.test/frame.html",
    "https://app.test",
  );
  expect(posted).toEqual([{ message: { event: "ready" }, origin: "https://app.test" }]);
  const call = { id: 1, method: "save", args: ["rsk"] };
  // Another page that got hold of the frame's window is not answered.
  await listeners[0]({ source: parent, origin: "https://other.test", data: call });
  expect(posted).toHaveLength(1);
  await listeners[0]({ source: parent, origin: "https://app.test", data: call });
  expect(posted[1]).toEqual({
    message: { id: 1, result: "saved rsk" },
    origin: "https://app.test",
  });
});

test("the frame loads fonts only from its own origin or the embedding page's", async () => {
  const fonts = {
    fonts: [
      "https://app.test/fonts/a.ttf",
      "https://frames.test/b.ttf",
      "https://elsewhere.test/c.ttf",
    ],
    cjkFont: "https://elsewhere.test/cjk.otf",
  };
  const { posted, mounted } = await run(
    `https://frames.test/frame.html?fonts=${encodeURIComponent(JSON.stringify(fonts))}`,
    "https://app.test",
  );
  expect(mounted?.fonts).toEqual(["https://app.test/fonts/a.ttf", "https://frames.test/b.ttf"]);
  expect(mounted?.cjkFont).toBe(false);
  expect(posted.filter(({ message }) => message.event === "fonterror")).toEqual(
    ["https://elsewhere.test/c.ttf", "https://elsewhere.test/cjk.otf"].map((font) => ({
      message: {
        event: "fonterror",
        error: `The font ${font} is not on this page's origin, so it was not loaded`,
      },
      origin: "https://app.test",
    })),
  );
});
