// Enforces the checkable rules of docs/README.md:
// 1. every file under docs/images, and every image elsewhere under docs/, is referenced;
// 2. docs/ holds only Markdown files at its top level;
// 3. Rust code never names a non-Markdown file under docs/ (test inputs live in tests/fixtures/).
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const image = /\.(?:avif|gif|jpe?g|png|svg|webp)$/i;
const pages = /\.(?:md|mdx|html)$/i;
const code = /\.(?:rs|mjs|py|ts)$/i;
const mainBlob = "https://github.com/Ameyanagi/ReShiki/blob/main/";

// Tracked files plus new files that are not ignored, so a local run sees them before `git add`.
const files = execFileSync(
  "git",
  ["ls-files", "-z", "--cached", "--others", "--exclude-standard"],
  { cwd: root, encoding: "utf8" },
)
  .split("\0")
  .filter((file) => file && existsSync(path.join(root, file)));
const assets = new Set(
  files.filter(
    (file) => file.startsWith("docs/images/") || (file.startsWith("docs/") && image.test(file)),
  ),
);
const referenced = new Set();
const errors = [];

function decode(value) {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

// A link target as a repository path, or null for external links and anchors.
function repositoryPath(file, href) {
  const target = decode(href.split(/[?#]/, 1)[0]);
  if (target.startsWith(mainBlob)) return path.posix.normalize(target.slice(mainBlob.length));
  if (!target || /^(?:[a-z][a-z\d+.-]*:|\/)/i.test(target)) return null;
  return path.posix.join(path.posix.dirname(file), target);
}

function linkTargets(body) {
  return [
    ...body.matchAll(/\]\(\s*<?([^)\s>]+)/g),
    ...body.matchAll(/^ {0,3}\[[^\]\n]+\]:\s*<?([^\s>]+)/gm),
    ...body.matchAll(/\b(?:src|href)\s*=\s*["']([^"'\n]+)["']/gi),
  ].map((match) => match[1]);
}

for (const file of files) {
  if (/^docs\/[^/]+$/.test(file) && !file.endsWith(".md")) {
    errors.push(`${file}: only Markdown pages belong directly under docs/.`);
  }
  const website = file.startsWith("website/src/");
  if (!pages.test(file) && !code.test(file) && !website) continue;
  const body = readFileSync(path.join(root, file), "utf8");
  if (pages.test(file)) {
    for (const href of linkTargets(body)) referenced.add(repositoryPath(file, href));
  }
  if (website) {
    for (const match of body.matchAll(/\/doc-images\/([^"'`()\s?#<>]+)/g)) {
      referenced.add(path.posix.normalize(`docs/images/${decode(match[1])}`));
    }
  }
  if (!code.test(file)) continue;
  const literals = [...body.matchAll(/["'`]([^"'`\n]*\bdocs\/[^"'`\n]*)["'`]/g)].map((m) => m[1]);
  if (file.endsWith(".rs")) {
    // concat!("…/docs/", "input.rsk") builds one path from several literals.
    for (const [call] of body.matchAll(/\bconcat!\((?:[^()]|\([^()]*\))*\)/g)) {
      literals.push([...call.matchAll(/"([^"\n]*)"/g)].map((m) => m[1]).join(""));
    }
  }
  for (const literal of literals) {
    if (literal.includes("://") || !/\bdocs\//.test(literal)) continue;
    const named = literal.slice(literal.search(/\bdocs\//));
    referenced.add(named);
    if (file.endsWith(".rs") && /\.\w+$/.test(named) && !named.endsWith(".md")) {
      errors.push(`${file}: names ${named}; move test inputs to tests/fixtures/.`);
    }
  }
}
for (const asset of assets) {
  if (!referenced.has(asset)) errors.push(`${asset}: nothing links it; link it or remove it.`);
}
if (errors.length) {
  console.error(
    `Documentation tree check failed:\n${errors
      .sort()
      .map((error) => `- ${error}`)
      .join("\n")}`,
  );
  process.exitCode = 1;
} else {
  console.log(`Documentation tree is clean: ${assets.size} referenced docs assets.`);
}
