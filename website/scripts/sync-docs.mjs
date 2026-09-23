import { cp, mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = fileURLToPath(new URL("../../", import.meta.url));
const source = path.join(root, "docs");
const content = path.join(root, "website/src/content/docs");
const destination = path.join(content, "developer");
const publishedImages = path.join(root, "website/public/doc-images");
await rm(publishedImages, { recursive: true, force: true });
await cp(path.join(source, "images"), publishedImages, { recursive: true });
const guides = {
  install: ["getting-started"],
  "first-molecule": ["bond-tools", "chain-tools", "tool-palettes"],
  templates: ["ring-presets", "template-library", "template-placement", "fragment-joining"],
  editing: [
    "contextual-shortcuts",
    "selection-and-groups",
    "selection-transforms",
    "selection-cleanup",
  ],
  labels: ["abbreviations", "atom-labels", "typography"],
  reactions: ["arrows", "reactions"],
  assistant: ["assistant"],
  figures: [
    "drawing-styles",
    "graphics",
    "jacs-style",
    "pictures",
    "publication-pages",
    "scientific-symbols",
  ],
  sharing: ["clipboard"],
};
const guideForTopic = new Map(
  Object.entries(guides).flatMap(([guide, topics]) => topics.map((topic) => [topic, guide])),
);
const historicalTopics = new Set([
  "capabilities",
  "changes-0.2",
  "framework-research",
  "selection-and-ring-placement",
  "workspace-review",
]);
// These directories contain generated pages only. Retire the old copies so
// their routes can be served by the redirects in astro.config.mjs.
await rm(path.join(content, "reference"), { recursive: true, force: true });
await rm(destination, { recursive: true, force: true });
await mkdir(destination, { recursive: true });
for (const name of (await readdir(source)).filter((name) => name.endsWith(".md")).sort()) {
  let body = await readFile(path.join(source, name), "utf8");
  const title = /^# (.+)$/m.exec(body)?.[1];
  if (!title) throw new Error(`Missing page title in ${name}`);
  body = body.replace(/^# .+\r?\n/m, "");
  body = body.replace(/\]\(([^\s)]+)([^)]*)\)/g, (match, href, suffix) => {
    if (/^(?:[a-z]+:|#|\/)/i.test(href)) return match;
    const [file, anchor] = href.split("#");
    const resolved = path.resolve(source, file);
    const fragment = anchor ? `#${anchor}` : "";
    if (path.dirname(resolved) === source && file.endsWith(".md")) {
      return `](/developer/${path.basename(file, ".md")}/${fragment}${suffix})`;
    }
    const relative = path.relative(root, resolved).split(path.sep).join("/");
    if (relative.startsWith("docs/images/")) {
      return `](/doc-images/${relative.slice("docs/images/".length)}${fragment}${suffix})`;
    }
    return `](https://github.com/Ameyanagi/ReShiki/blob/main/${relative}${fragment}${suffix})`;
  });
  const topic = path.basename(name, ".md");
  const pagination = topic === "development" ? "prev: false\n" : "";
  const guide = guideForTopic.get(topic);
  // Keep archived implementation checks from outranking the visual manual.
  const search = guide || historicalTopics.has(topic) ? "pagefind: false\n" : "";
  const note = guide
    ? `\nImplementation notes and historical checks. [Open the visual guide →](/guide/${guide}/).\n`
    : historicalTopics.has(topic)
      ? "\nEngineering notes and historical checks.\n"
      : "";
  await writeFile(
    path.join(destination, name),
    `---\ntitle: ${JSON.stringify(title)}\nslug: developer/${topic}\n${pagination}${search}editUrl: https://github.com/Ameyanagi/ReShiki/edit/main/docs/${name}\n---\n${note}${body}`,
  );
}
