import { cp, mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { historicalTopics } from "./doc-topics.mjs";

const root = fileURLToPath(new URL("../../", import.meta.url));
const source = path.join(root, "docs");
const content = path.join(root, "website/src/content/docs");
const destination = path.join(content, "developer");
const publishedImages = path.join(root, "website/public/doc-images");
await mkdir(path.join(root, "website/public/examples"), { recursive: true });
await cp(
  path.join(root, "assets/examples/shortcut-examples.rsk"),
  path.join(root, "website/public/examples/shortcut-examples.rsk"),
);
const guides = {
  install: ["getting-started"],
  "first-molecule": ["bond-tools", "chain-tools", "tool-palettes"],
  templates: [
    "haworth-projections",
    "ring-presets",
    "template-library",
    "template-placement",
    "fragment-joining",
    "porphine-core",
  ],
  editing: [
    "contextual-shortcuts",
    "selection-and-groups",
    "selection-transforms",
    "selection-cleanup",
    "reference-geometry",
    "rear-opacity",
    "mol-import-3d",
  ],
  labels: ["abbreviations", "atom-labels", "typography"],
  names: ["chemical-naming-rust"],
  nmr: ["nmr-prediction"],
  "generic-structures": ["generic-structures"],
  reactions: ["arrows", "reactions"],
  assistant: ["assistant"],
  "assistant-setup": ["assistant-setup"],
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
const historicalTopicSet = new Set(historicalTopics);
// These directories contain generated pages only. Retire the old copies so
// their routes can be served by the redirects in astro.config.mjs.
await rm(path.join(content, "reference"), { recursive: true, force: true });
await rm(destination, { recursive: true, force: true });
await mkdir(destination, { recursive: true });
for (const name of (await readdir(source))
  .filter((name) => name.endsWith(".md") && name !== "README.md")
  .sort()) {
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
    if (relative === "assets/examples/shortcut-examples.rsk") {
      return `](/examples/shortcut-examples.rsk${suffix})`;
    }
    if (relative.startsWith("docs/images/")) {
      return `](/doc-images/${relative.slice("docs/images/".length)}${fragment}${suffix})`;
    }
    return `](https://github.com/Ameyanagi/ReShiki/blob/main/${relative}${fragment}${suffix})`;
  });
  const topic = path.basename(name, ".md");
  if (topic === "contextual-shortcuts") {
    await writeFile(
      path.join(content, "guide/shortcuts.md"),
      `---\ntitle: Shortcuts and examples\ndescription: A complete keyboard reference and one editable ReShiki document of examples.\neditUrl: https://github.com/Ameyanagi/ReShiki/edit/main/docs/contextual-shortcuts.md\n---\n${body}`,
    );
  }
  if (topic === "assistant-setup") {
    const guideBody = body.replace(/\]\(([^\s)]+)([^)]*)\)/g, (_match, href, suffix) => {
      const localHref = href.replace(/^https:\/\/reshiki\.com\//, "/");
      const developerLink = /^\/developer\/([^/]+)\/(#.*)?$/.exec(localHref);
      const guide = developerLink && guideForTopic.get(developerLink[1]);
      return `](${guide ? `/guide/${guide}/${developerLink[2] ?? ""}` : localHref}${suffix})`;
    });
    await writeFile(
      path.join(content, "guide/assistant-setup.md"),
      `---\ntitle: Set up Codex\ndescription: Install Codex, sign in, and connect ReShiki’s optional AI assistant.\neditUrl: https://github.com/Ameyanagi/ReShiki/edit/main/docs/assistant-setup.md\n---\n${guideBody}`,
    );
  }
  const pagination = topic === "development" ? "prev: false\n" : "";
  const guide = guideForTopic.get(topic);
  // Keep archived implementation checks from outranking the visual manual.
  const search = guide || historicalTopicSet.has(topic) ? "pagefind: false\n" : "";
  const note = guide
    ? `\nImplementation notes and historical checks. [Open the visual guide →](/guide/${guide}/).\n`
    : historicalTopicSet.has(topic)
      ? "\nEngineering notes and historical checks.\n"
      : "";
  await writeFile(
    path.join(destination, name),
    `---\ntitle: ${JSON.stringify(title)}\nslug: developer/${topic}\n${pagination}${search}editUrl: https://github.com/Ameyanagi/ReShiki/edit/main/docs/${name}\n---\n${note}${body}`,
  );
}

async function filesUnder(directory) {
  const files = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const file = path.join(directory, entry.name);
    if (entry.isDirectory()) files.push(...(await filesUnder(file)));
    else if (entry.isFile()) files.push(file);
  }
  return files;
}

const images = path.join(source, "images");
const sourceImages = new Set(await filesUnder(images));
const referencedImages = new Set();
for (const file of await filesUnder(path.join(root, "website/src"))) {
  if (!/\.(?:md|mdx|astro|ts|mjs)$/.test(file)) continue;
  const body = await readFile(file, "utf8");
  for (const match of body.matchAll(/\/doc-images\/([^"'`()\s?#<>]+)/g)) {
    // Only files listed under docs/images are copied, so a "../" reference cannot escape it.
    const image = path.resolve(images, decodeURIComponent(match[1]));
    if (sourceImages.has(image)) referencedImages.add(image);
  }
}
await rm(publishedImages, { recursive: true, force: true });
await mkdir(publishedImages, { recursive: true });
for (const image of [...referencedImages].sort()) {
  const target = path.join(publishedImages, path.relative(images, image));
  await mkdir(path.dirname(target), { recursive: true });
  await cp(image, target);
}
console.log(
  `Copied ${referencedImages.size} of ${sourceImages.size} docs/images files referenced by the site.`,
);
