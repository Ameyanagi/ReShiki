import catalog from "../data/theme-colors.json";

export { catalog };
export type Color = { rgb: number[]; oklch: number[] };
export type Theme = (typeof catalog.themes)[number];
export type Row = "strong" | "tint";
export const modes = ["light", "dark"] as const;
export const rows = ["strong", "tint"] as const;
export const hex = (color: Color) =>
  `#${color.rgb
    .map((n) => n.toString(16).padStart(2, "0"))
    .join("")
    .toUpperCase()}`;
export const rgb = (color: Color) => `rgb(${color.rgb.join(" ")})`;
export const oklch = (color: Color) => `oklch(${color.oklch.join(" ")})`;

type Entry = [role: string, name: string, number: number | "", variable: string, color: Color];

/** Paper, ink, the Strong and Tint rows, then element labels, as the downloads list them. */
function colors(theme: Theme, mode: (typeof modes)[number]): Entry[] {
  const palette = theme.modes[mode];
  return [
    ["paper", "paper", "", "paper", palette.paper],
    ["ink", "ink", "", "ink", palette.ink],
    ...rows.flatMap((row) =>
      palette[row].colors.map((color): Entry => [
        row,
        color.hue,
        "",
        `${row}-${color.hue.toLowerCase()}`,
        color,
      ]),
    ),
    ...palette.elements.map((element): Entry => [
      "label",
      element.symbol,
      element.number,
      `element-${element.symbol}`,
      element.label,
    ]),
  ];
}

export function csv() {
  const table: (string | number)[][] = [
    [
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
    ],
  ];
  for (const theme of catalog.themes)
    for (const mode of modes)
      for (const [role, name, number, , color] of colors(theme, mode))
        table.push([theme.id, mode, role, name, number, hex(color), ...color.rgb, ...color.oklch]);
  return (
    table
      .map((row) => row.map((value) => `"${String(value).replaceAll('"', '""')}"`).join(","))
      .join("\r\n") + "\r\n"
  );
}

export function css() {
  const lines = [
    "/* ReShiki palette reference. RGB is exact; OKLCH is rounded to six decimals.",
    " * Sources and context: https://reshiki.com/guide/color-palettes/",
    " * Set theme and mode on the same element. */",
  ];
  for (const theme of catalog.themes)
    for (const mode of modes) {
      lines.push(`\n[data-reshiki-theme="${theme.id}"][data-reshiki-mode="${mode}"] {`);
      for (const [, , , name, color] of colors(theme, mode))
        lines.push(
          `  --reshiki-${name}: ${rgb(color)};`,
          `  --reshiki-${name}-oklch: ${oklch(color)};`,
        );
      lines.push("}");
    }
  return lines.join("\n") + "\n";
}
