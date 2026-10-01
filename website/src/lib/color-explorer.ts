import { hex, rgb, oklch, modes, rows, type Color, type Row, type Theme } from "./color-reference";

export type Mode = "light" | "dark";
export type Selection =
  | { kind: "element"; index: number }
  | { kind: "ink"; mode: Mode }
  | { kind: "swatch"; mode: Mode; row: Row; index: number };
const escape = (value: string) => value.replace(/[&<>"']/g, (char) => `&#${char.charCodeAt(0)};`);
const copyIcon =
  '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V4a1 1 0 0 0-1-1H4a1 1 0 0 0-1 1v11a1 1 0 0 0 1 1h4"/></svg>';
const title = (mode: Mode) => (mode === "light" ? "Light" : "Dark");
const rowName = (row: Row) => (row === "strong" ? "Strong" : "Tint");
const fixed = (value: number, digits: number) => value.toFixed(digits);

function position(number: number): [number, number] {
  if (number === 1) return [1, 1];
  if (number === 2) return [1, 18];
  if (number <= 10) return [2, number <= 4 ? number - 2 : number + 8];
  if (number <= 18) return [3, number <= 12 ? number - 10 : number];
  if (number <= 36) return [4, number - 18];
  if (number <= 54) return [5, number - 36];
  if (number <= 57) return [6, number - 54];
  if (number <= 71) return [9, number - 54];
  if (number <= 86) return [6, number - 68];
  if (number <= 89) return [7, number - 86];
  if (number <= 103) return [10, number - 86];
  return [7, number - 100];
}

export function paletteTiles(theme: Theme, selected: Selection, mode: Mode) {
  return theme.modes[mode].elements
    .map((element, index) => {
      const [row, col] = position(element.number);
      const active = selected.kind === "element" && selected.index === index;
      const tabStop = active || (selected.kind !== "element" && index === 0);
      const color = hex(element.label);
      return `<button class="cr-tile" type="button" tabindex="${tabStop ? 0 : -1}" data-element="${index}" data-symbol="${element.symbol.toLowerCase()}" data-number="${element.number}" style="grid-row:${row};grid-column:${col};--element-color:${color}" aria-label="${element.symbol}, element ${element.number}" aria-pressed="${active}"><span class="cr-number">${element.number}</span><span class="cr-symbol">${element.symbol}</span><span class="cr-color-swatch" style="background:${color}" aria-hidden="true"></span></button>`;
    })
    .join("");
}

/** The theme card strip: the Strong row on the chosen canvas. */
export function themeStrip(theme: Theme, mode: Mode) {
  return theme.modes[mode].strong.colors
    .map((color) => `<span style="background:${hex(color)}"></span>`)
    .join("");
}

const swatchKey = (mode: Mode, row: Row | "ink", index = 0) =>
  row === "ink" ? `${mode}/ink` : `${mode}/${row}/${index}`;
export function selectedKey(selected: Selection) {
  if (selected.kind === "ink") return swatchKey(selected.mode, "ink");
  if (selected.kind === "swatch") return swatchKey(selected.mode, selected.row, selected.index);
  return "";
}

export function parseSwatch(key: string): Selection {
  const [mode, row, index] = key.split("/") as [Mode, Row | "ink", string?];
  return row === "ink"
    ? { kind: "ink", mode }
    : { kind: "swatch", mode, row, index: Number(index) };
}

/** Strong (Ink + eight hues) and Tint (eight hues) on one canvas, as the color popover shows them. */
export function paperChips(theme: Theme, selected: Selection, mode: Mode) {
  const palette = theme.modes[mode];
  const current = selectedKey(selected);
  const keys = [
    swatchKey(mode, "ink"),
    ...rows.flatMap((row) => palette[row].colors.map((_, i) => swatchKey(mode, row, i))),
  ];
  const stop = keys.includes(current) ? current : keys[0];
  const chip = (key: string, color: Color, label: string) =>
    `<button class="cr-chip" type="button" tabindex="${key === stop ? 0 : -1}" data-swatch="${key}" style="background:${hex(color)}" title="${escape(label)}" aria-label="${escape(`${label}, ${title(mode).toLowerCase()} canvas`)}" aria-pressed="${key === current}"></button>`;
  const header = [
    "Ink",
    ...palette.strong.colors.map((color) => `${color.hue}<small>${color.degrees}°</small>`),
  ]
    .map((name) => `<span class="cr-hue-name" aria-hidden="true">${name}</span>`)
    .join("");
  const line = (row: Row) =>
    `<span class="cr-row-name">${rowName(row)}<small>L ${fixed(palette[row].lightness, 2)}</small></span>${
      row === "strong"
        ? chip(swatchKey(mode, "ink"), palette.ink, "Ink")
        : '<span aria-hidden="true"></span>'
    }${palette[row].colors.map((color, i) => chip(swatchKey(mode, row, i), color, `${color.hue} · ${rowName(row)}`)).join("")}`;
  return `<span class="cr-hue-name" aria-hidden="true"></span>${header}${line("strong")}${line("tint")}`;
}

function value(color: Color) {
  const [l, c, h] = color.oklch;
  return `<span class="cr-value" title="${oklch(color)}"><span class="cr-dot" style="background:${hex(color)}" aria-hidden="true"></span><code>${hex(color)}</code><span class="cr-lch">${fixed(l, 3)} ${fixed(c, 3)} ${fixed(h, 1)}</span></span>`;
}

/** Every palette value of a theme: paper, ink and both rows on both canvases. */
export function valuesTable(theme: Theme) {
  const head = modes
    .flatMap((mode) =>
      rows.map((row) => {
        const tone = theme.modes[mode][row];
        return `<th scope="col">${rowName(row)}<small>L ${fixed(tone.lightness, 2)} · C ${tone.target_chroma}</small></th>`;
      }),
    )
    .join("");
  const cells = (pick: (mode: Mode, row: Row) => Color | null) =>
    modes
      .flatMap((mode) =>
        rows.map((row) => {
          const color = pick(mode, row);
          return color
            ? `<td>${value(color)}</td>`
            : '<td><span class="cr-none" role="img" aria-label="None">—</span></td>';
        }),
      )
      .join("");
  const paper = modes
    .map((mode) => `<td colspan="2">${value(theme.modes[mode].paper)}</td>`)
    .join("");
  const hues = theme.modes.light.strong.colors
    .map(
      (color, i) =>
        `<tr><th scope="row">${color.hue}<small>${color.degrees}°</small></th>${cells((mode, row) => theme.modes[mode][row].colors[i])}</tr>`,
    )
    .join("");
  return `<table class="cr-values"><caption>${escape(theme.name)} · HEX, then OKLCH as L C h</caption><thead><tr><th scope="col" rowspan="2">Color</th><th scope="colgroup" colspan="2">Light canvas</th><th scope="colgroup" colspan="2">Dark canvas</th></tr><tr>${head}</tr></thead><tbody><tr><th scope="row">Paper</th>${paper}</tr><tr><th scope="row">Ink</th>${cells((mode, row) => (row === "strong" ? theme.modes[mode].ink : null))}</tr>${hues}</tbody></table>`;
}

export function selectedColor(theme: Theme, selected: Selection, mode: Mode): Color {
  if (selected.kind === "element") return theme.modes[mode].elements[selected.index].label;
  if (selected.kind === "ink") return theme.modes[selected.mode].ink;
  return theme.modes[selected.mode][selected.row].colors[selected.index];
}

export function inspector(theme: Theme, selected: Selection, mode: Mode) {
  const canvas = selected.kind === "element" ? mode : selected.mode;
  const color = selectedColor(theme, selected, mode);
  let heading: string, name: string, detail: string, sample: string;
  if (selected.kind === "element") {
    const element = theme.modes[mode].elements[selected.index];
    [heading, name, detail] = [`Element ${element.number}`, element.symbol, "Canvas color"];
    sample = `<span class="cr-preview-symbol" style="color:${hex(color)}">${element.symbol}</span>`;
  } else if (selected.kind === "ink") {
    [heading, name, detail] = ["Canvas", "Ink", "Shared by all themes"];
    sample = `<span class="cr-preview-symbol" style="color:${hex(color)}">Aa</span>`;
  } else {
    const swatch = theme.modes[canvas][selected.row].colors[selected.index];
    [heading, name] = [`${rowName(selected.row)} row`, swatch.hue];
    detail = `${swatch.degrees}° · L ${fixed(theme.modes[canvas][selected.row].lightness, 2)}`;
    sample =
      selected.row === "strong"
        ? `<span class="cr-preview-symbol" style="color:${hex(color)}">Aa</span>`
        : `<svg class="cr-ring-preview" viewBox="0 0 100 100" role="img" aria-label="${escape(name)} tint in a ring"><polygon points="50,10 85,30 85,70 50,90 15,70 15,30" fill="${hex(color)}" stroke="currentColor" stroke-width="2"/></svg>`;
  }
  const label = `${name}${selected.kind === "swatch" ? ` ${rowName(selected.row)}` : ""} ${canvas}`;
  return `<div class="cr-inspector-title"><span class="cr-eyebrow">${heading}</span><h2>${escape(name)}</h2><p>${escape(theme.name)} · ${detail}</p></div>
    <section class="cr-mode-card" aria-label="${escape(label)}"><div class="cr-preview" data-mode="${canvas}"><span class="cr-preview-caption">${title(canvas)} canvas</span>${sample}<span class="cr-preview-swatch" style="background:${hex(color)}" aria-hidden="true"></span></div><div class="cr-code-list">${[
      ["HEX", hex(color)],
      ["RGB", rgb(color)],
      ["OKLCH", oklch(color)],
    ]
      .map(
        ([format, value]) =>
          `<button type="button" data-copy="${escape(value)}" aria-label="Copy ${escape(label)} ${format}"><span>${format}</span><code>${escape(value)}</code>${copyIcon}</button>`,
      )
      .join("")}</div></section>`;
}
