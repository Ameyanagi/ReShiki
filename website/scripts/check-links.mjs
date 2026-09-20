import { access, readFile, readdir } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
const output = fileURLToPath(new URL("../dist/", import.meta.url));
let checked = 0;
async function visit(directory) {
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const file = path.join(directory, entry.name);
    if (entry.isDirectory()) await visit(file);
    else if (entry.name.endsWith(".html")) {
      const html = await readFile(file, "utf8");
      for (const match of html.matchAll(/(?:href|src)="(\/(?!\/)[^"?#]*)(?:[?#][^"]*)?"/g)) {
        let target = path.join(output, decodeURIComponent(match[1].slice("/".length)));
        if (target.endsWith(path.sep)) target += "index.html";
        await access(target).catch(() => {
          throw new Error(`Broken link in ${file}: ${match[1]}`);
        });
        checked++;
      }
    }
  }
}
await visit(output);
console.log(`Verified ${checked} local documentation links and assets.`);
