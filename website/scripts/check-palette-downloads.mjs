// Validate the deployed formats independently of their serializers. The Rust
// theme_reference test separately checks this catalog against the application.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const read = (path) => readFile(new URL(path, import.meta.url), "utf8");
const catalog = JSON.parse(await read("../src/data/theme-colors.json"));
assert.deepEqual(JSON.parse(await read("../dist/colors/palettes.json")), catalog);

const rows = (await read("../dist/colors/palettes.csv"))
  .trim()
  .split(/\r?\n/)
  .map((line) => {
    assert.match(line, /^"(?:[^"]|"")*"(?:,"(?:[^"]|"")*")*$/);
    return [...line.matchAll(/"((?:[^"]|"")*)"(?:,|$)/g)].map((cell) =>
      cell[1].replaceAll('""', '"'),
    );
  });
assert.deepEqual(rows.shift(), [
  "theme",
  "mode",
  "role",
  "name",
  "atomic_number",
  "hex",
  "red",
  "green",
  "blue",
  "oklch_l",
  "oklch_c",
  "oklch_h_degrees",
]);
const csvRows = new Map(rows.map((row) => [row.slice(0, 4).join("/"), row]));
assert.equal(csvRows.size, rows.length, "Duplicate CSV colors");

const css = await read("../dist/colors/palettes.css");
const blocks = [
  ...css.matchAll(/\[data-reshiki-theme="([^"]+)"\]\[data-reshiki-mode="([^"]+)"\]\s*\{([^}]+)\}/g),
];
const cssBlocks = new Map(blocks.map((block) => [block.slice(1, 3).join("/"), block[3]]));
assert.equal(blocks.length, 6, "All three themes must include both modes");
assert.equal(cssBlocks.size, blocks.length, "Duplicate CSS palettes");

let checked = 0;
for (const theme of catalog.themes) {
  for (const mode of ["light", "dark"]) {
    const palette = theme.modes[mode];
    const block = cssBlocks.get(`${theme.id}/${mode}`);
    assert(block, `Missing CSS palette: ${theme.id}/${mode}`);
    const entries = [...block.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)];
    const variables = new Map(entries.map((entry) => [entry[1], entry[2].trim()]));
    assert.equal(variables.size, entries.length, "Duplicate CSS variables");
    const colors = [
      ["paper", "paper", "", "paper", palette.paper],
      ["ink", "ink", "", "ink", palette.ink],
      ...Object.entries(palette.ring_fills).map(([name, color]) => [
        "ring_fill",
        name,
        "",
        `ring-${name.toLowerCase()}`,
        color,
      ]),
      ...palette.elements.map((element) => [
        "label",
        element.symbol,
        String(element.number),
        `element-${element.symbol}`,
        element.label,
      ]),
    ];
    assert.equal(variables.size, colors.length * 2, "Missing or extra CSS colors");
    for (const [role, name, number, variable, color] of colors) {
      const key = `${theme.id}/${mode}/${role}/${name}`;
      const row = csvRows.get(key);
      assert(row, `Missing CSV color: ${key}`);
      assert.equal(row.length, 12);
      assert.equal(row[4], number, key);
      assert.match(row[5], /^#[0-9A-F]{6}$/);
      assert.deepEqual(
        row[5]
          .slice(1)
          .match(/../g)
          .map((n) => parseInt(n, 16)),
        color.rgb,
        key,
      );
      assert.deepEqual(row.slice(6, 9).map(Number), color.rgb, key);
      assert.deepEqual(row.slice(9).map(Number), color.oklch, key);
      assert.equal(variables.get(`--reshiki-${variable}`), `rgb(${color.rgb.join(" ")})`, key);
      assert.equal(
        variables.get(`--reshiki-${variable}-oklch`),
        `oklch(${color.oklch.join(" ")})`,
        key,
      );
      checked++;
    }
  }
}
assert.equal(rows.length, checked, "Missing or extra CSV colors");
console.log(`Verified ${checked} colors in the JSON, CSV and CSS downloads.`);
