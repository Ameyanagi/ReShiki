import { hex, rgb, oklch, type Color, type Theme } from "./color-reference";

export type Mode = "light" | "dark";
export type Selection = { kind: "element"; index: number } | { kind: "role"; name: string };
const escape = (value: string) => value.replace(/[&<>"']/g, (char) => `&#${char.charCodeAt(0)};`);
const copyIcon =
  '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V4a1 1 0 0 0-1-1H4a1 1 0 0 0-1 1v11a1 1 0 0 0 1 1h4"/></svg>';

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
      const tabStop = active || (selected.kind === "role" && index === 0);
      const color = hex(element.label);
      return `<button class="cr-tile" type="button" tabindex="${tabStop ? 0 : -1}" data-element="${index}" data-symbol="${element.symbol.toLowerCase()}" data-number="${element.number}" style="grid-row:${row};grid-column:${col};--element-color:${color}" aria-label="${element.symbol}, element ${element.number}" aria-pressed="${active}"><span class="cr-number">${element.number}</span><span class="cr-symbol">${element.symbol}</span><span class="cr-color-swatch" style="background:${color}" aria-hidden="true"></span></button>`;
    })
    .join("");
}

export function selectedColor(theme: Theme, selected: Selection, mode: Mode): Color {
  const palette = theme.modes[mode];
  if (selected.kind === "element") return palette.elements[selected.index].label;
  if (selected.name === "paper" || selected.name === "ink") return palette[selected.name];
  return palette.ring_fills[selected.name as keyof typeof palette.ring_fills];
}

export function inspector(theme: Theme, selected: Selection, mode: Mode) {
  const element = selected.kind === "element" ? theme.modes[mode].elements[selected.index] : null;
  const name = element ? element.symbol : selected.kind === "role" ? selected.name : "";
  const heading = element
    ? `Element ${element.number}`
    : ["paper", "ink"].includes(name)
      ? "Canvas"
      : "Ring fill";
  const color = selectedColor(theme, selected, mode);
  const label = `${name} ${mode}`;
  const title = mode === "light" ? "Light" : "Dark";
  const sample =
    selected.kind === "role" && !["paper", "ink"].includes(name)
      ? `<svg class="cr-ring-preview" viewBox="0 0 100 100" aria-label="${escape(name)} ring fill"><polygon points="50,10 85,30 85,70 50,90 15,70 15,30" fill="${hex(color)}" stroke="currentColor" stroke-width="2"/></svg>`
      : `<span class="cr-preview-symbol" style="color:${name === "paper" ? "inherit" : hex(color)}">${element ? name : "Aa"}</span>`;
  return `<div class="cr-inspector-title"><span class="cr-eyebrow">${heading}</span><h2>${escape(name === "paper" ? "Paper" : name === "ink" ? "Ink" : name)}</h2><p>${theme.name} · ${selected.kind === "element" ? "Canvas color" : heading === "Canvas" ? "Shared by all themes" : "This theme's ring tint"}</p></div>
    <section class="cr-mode-card" aria-label="${escape(label)}"><div class="cr-preview" data-mode="${mode}"><span class="cr-preview-caption">${title} canvas</span>${sample}<span class="cr-preview-swatch" style="background:${hex(color)}" aria-hidden="true"></span></div><div class="cr-code-list">${[
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

export function supportingTiles(theme: Theme, mode: Mode) {
  return ["paper", "ink", ...Object.keys(theme.modes[mode].ring_fills)]
    .map((name) => {
      const selection: Selection = { kind: "role", name };
      return `<button type="button" class="cr-support-tile" data-support="${name}" aria-pressed="false"><span class="cr-color-swatch" style="background:${hex(selectedColor(theme, selection, mode))}" aria-hidden="true"></span><span>${name === "paper" ? "Paper" : name === "ink" ? "Ink" : name}</span></button>`;
    })
    .join("");
}
