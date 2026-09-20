import { mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = fileURLToPath(new URL("../../", import.meta.url));
const source = path.join(root, "docs");
const destination = path.join(root, "website/src/content/docs/reference");
await rm(destination, { recursive: true, force: true });
await mkdir(destination, { recursive: true });
for (const name of (await readdir(source)).filter((name) => name.endsWith(".md"))) {
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
      return `](/moruno/reference/${path.basename(file, ".md")}/${fragment}${suffix})`;
    }
    const relative = path.relative(root, resolved).split(path.sep).join("/");
    return `](https://github.com/Ameyanagi/moruno/blob/main/${relative}${fragment}${suffix})`;
  });
  await writeFile(
    path.join(destination, name),
    `---\ntitle: ${JSON.stringify(title)}\nslug: reference/${path.basename(name, ".md")}\neditUrl: https://github.com/Ameyanagi/moruno/edit/main/docs/${name}\n---\n${body}`,
  );
}
