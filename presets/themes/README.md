# ReShiki theme library

A theme controls canvas element colors, periodic-table tile seeds, and five ring-highlight slots. It is independent of a journal's drawing style (fonts, bond lengths, stroke widths). The interface's Match canvas / Light / Dark preference remains independent. In v1, canvas paper stays white or black and neutral bonds stay black or white.

Use **Theme → Export theme…** to export the selected palette's light and dark definitions. **Import theme…** validates a file, applies it with Undo, and stores it in the application's data directory under `themes/<id>.reshiki-theme`. The drawing embeds a snapshot, including inherited values, so a library update does not change saved drawings. Explicit pasted/object colors remain independent; choosing a theme deliberately resets atom overrides.

## Format

Files are UTF-8 JSON, limited to 256 KB. [schema.json](schema.json) describes version 1. Both `light` and `dark` are required. [soft-jmol.reshiki-theme](soft-jmol.reshiki-theme) is a small working example that mixes RGB and OKLCH authoring.

| Field                         | Meaning                                                            |
| ----------------------------- | ------------------------------------------------------------------ |
| `version`                     | `1`; unsupported versions are rejected                             |
| `id`                          | Stable ID: 1–64 lowercase ASCII letters, digits or hyphens         |
| `name`                        | Picker label, up to 80 printable characters                        |
| `author`, `source`, `license` | Optional credits; required for contributed third-party material    |
| `base`                        | Built-in fallback: `publication`, `presentation`, `pastel`, `jmol` |
| `light`, `dark`               | Independent palettes with the roles below                          |
| `elements`                    | Element symbols mapped to automatic label color seeds              |
| `tile_seeds`                  | Element symbols mapped to tile background hue/chroma seeds         |
| `ring_fills`                  | Named RGB/OKLCH colors for `Sky`, `Mint`, `Rose`, `Lilac`, `Sand`  |

A color is either an sRGB triplet, e.g. `[119, 150, 210]`, or `{ "oklch": [0.76, 0.10, 265] }`: lightness 0–1, chroma 0–0.4, hue 0–360 **degrees**. Do not store RGB and OKLCH as competing sources for the same color. OKLCH inputs are mapped to sRGB by reducing chroma while retaining hue/lightness. Runtime contrast checks use the resulting 8-bit RGB values.

Missing roles inherit from `base`. A partial file is convenient for authoring; imported drawings and exported themes freeze those inherited roles. RGB exports of built-ins preserve exact color seeds. Authored OKLCH overrides remain OKLCH when re-exported. Label colors can move in lightness to remain readable; `elements` is not an instruction to bypass contrast checks. Tile states are generated separately, with neutral, regular-weight symbols.

Ring fills must contrast with neutral ink by at least 5:1 in their mode. This provides a feasible foreground for automatic labels even when several fills meet. Invalid elements, unknown roles, invalid numeric ranges and low-contrast fills are rejected before application. The JSON schema checks structure; the Rust validator also checks color contrast. Explicit custom artwork is never silently recolored.

## Validate and contribute

1. Export an existing theme or copy the example. Give it a unique ID and name; edit both palettes.
2. Run `cargo run --locked --example theme_library -- --check path/to/theme.reshiki-theme`. It checks all 118 element labels against paper and all five overlapping ring fills in both modes.
3. Import it in ReShiki. Inspect small labels, filled rings, selected tiles, and mixed canvas/interface modes. Check projected appearance and color-vision/grayscale views as well as numerical contrast. Symbols must remain meaningful without color.
4. Submit the file under `presets/themes/` in a GitHub PR, with credits, license information and links to review images showing both modes. Follow [CONTRIBUTING.md](../../CONTRIBUTING.md). Do not claim rights to someone else's palette. Temporary screenshots can be attached to the PR rather than committed.
5. To include an accepted theme in releases, add its `include_str!` entry to `theme_files::bundled()` in `src/theme_files.rs`. The bundled-theme regression tests exercise every registered entry. No executable code or network downloads are carried by a theme file.

`cargo run --locked --example theme_library -- /tmp/reshiki-theme-library` exports the built-ins, example theme, and journal styles (native and CDS) for review. This creates no repository artifacts.

## Sources

Element identity starts from the [Jmol reference table](https://jmol.sourceforge.net/jscolors/) (H–Mt; later elements remain neutral). The original theme adjustments are ReShiki contributions; the reference data is credited separately. OKLab conversion matrices follow Björn Ottosson's [public-domain implementation, updated 2021-01-25](https://bottosson.github.io/posts/oklab/). Text uses a 5:1 generation target and a 4.5:1 minimum; meaningful outlines use a 3.2:1 target. These use [WCAG contrast](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html) as an engineering baseline, not a universal accessibility certification for arbitrary artwork or paste backgrounds.
