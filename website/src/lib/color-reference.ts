import catalog from "../data/theme-colors.json";

export { catalog };
export type Color = { rgb: number[]; oklch: number[] };
export type Theme = (typeof catalog.themes)[number];
export const modes = ["light", "dark"] as const;
export const hex = (color: Color) =>
  `#${color.rgb
    .map((n) => n.toString(16).padStart(2, "0"))
    .join("")
    .toUpperCase()}`;
export const rgb = (color: Color) => `rgb(${color.rgb.join(" ")})`;
export const oklch = (color: Color) => `oklch(${color.oklch.join(" ")})`;

export function csv() {
  const rows: (string | number)[][] = [
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
    for (const mode of modes) {
      const add = (role: string, name: string, number: number | string, color: Color) => {
        rows.push([theme.id, mode, role, name, number, hex(color), ...color.rgb, ...color.oklch]);
      };
      const palette = theme.modes[mode];
      add("paper", "paper", "", palette.paper);
      add("ink", "ink", "", palette.ink);
      for (const [name, color] of Object.entries(palette.ring_fills))
        add("ring_fill", name, "", color);
      for (const element of palette.elements) {
        add("label", element.symbol, element.number, element.label);
      }
    }
  return (
    rows
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
      const add = (name: string, color: Color) => {
        lines.push(
          `  --reshiki-${name}: ${rgb(color)};`,
          `  --reshiki-${name}-oklch: ${oklch(color)};`,
        );
      };
      const palette = theme.modes[mode];
      add("paper", palette.paper);
      add("ink", palette.ink);
      for (const [name, color] of Object.entries(palette.ring_fills))
        add(`ring-${name.toLowerCase()}`, color);
      for (const element of palette.elements) {
        add(`element-${element.symbol}`, element.label);
      }
      lines.push("}");
    }
  return lines.join("\n") + "\n";
}
