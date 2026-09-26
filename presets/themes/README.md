# ReShiki theme library

A theme controls canvas element colors, periodic-table tile seeds, and five ring-highlight slots. It is independent of a journal's drawing style (fonts, bond lengths, stroke widths). The interface's Match canvas / Light / Dark preference remains independent. In v1, canvas paper stays white or black and neutral bonds stay black or white.

Open **Theme → Manage themes…**. The top-right **Import**, **Save**, and **Export** icons sit beside **B**. Import validates a file and opens both palettes for preview without changing the drawing or library. Save stores it under `themes/<id>.reshiki-theme` in the application's data directory and applies it with Undo. Export writes the preview without applying or adding it to the library. The drawing embeds a snapshot, including inherited values, so a library update does not change saved drawings. Explicit pasted/object colors remain independent; choosing a theme deliberately resets atom overrides.

Select a saved theme in the manager to edit it in place or use the trash icon to remove it. Publication, Presentation, Pastel, and Jmol are protected; saving a built-in creates a custom copy. **New theme…** starts a fresh draft with a unique name. Deleting only removes a library entry; existing drawings retain their embedded palettes. Deleted library files are archived under `themes/.deleted/` for recovery. The loader scans all saved theme files; unrelated directory entries do not limit the library. Soft Jmol is an optional example file, not automatically installed.

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
5. Accepted files can be distributed as optional imports; installing them is an explicit choice in the theme manager. `theme_files::bundled()` registers example files for validation and the export example, not automatic installation. No executable code or network downloads are carried by a theme file.

`cargo run --locked --example theme_library -- /tmp/reshiki-theme-library` exports the built-ins, example theme, and journal styles (native and CDS) for review. This creates no repository artifacts.

## Sources

Element identity starts from the [Jmol reference table](https://jmol.sourceforge.net/jscolors/) (H–Mt; later elements remain neutral). The original theme adjustments are ReShiki contributions; the reference data is credited separately. OKLab conversion matrices follow Björn Ottosson's [public-domain implementation, updated 2021-01-25](https://bottosson.github.io/posts/oklab/). Text uses a 5:1 generation target and a 4.5:1 minimum; meaningful outlines use a 3.2:1 target. These use [WCAG contrast](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html) as an engineering baseline, not a universal accessibility certification for arbitrary artwork or paste backgrounds.

## Generate a theme in ReShiki

Choose **Theme → Manage themes…**, then **New theme…**. Choose a **Reference theme** from Jmol, the built-in themes, or your imported/saved theme library, then set **Lightness** and **Color intensity** separately for light and dark mode. The Presentation and Pastel buttons load their standard settings and reset the reference to Jmol. Both light and dark periodic tables stay visible in the scrollable preview. The four preview icons under **Make it yours** select element labels, solid **Color tiles**, softened **Interface tiles**, or labels on ring fills; hover for a description. The **B** button at the top right toggles bold symbols. Color tiles use the final element color as their background and select black or white text for readability. Click an element to see its RGB colors and contrast. Each table includes a molecule preview with an NH group and a carbonyl, using the application's drawing renderer.

Generation converts the reference's element color seeds from RGB into OKLCH, preserves their hue, sets lightness directly to your target, and multiplies reference chroma by your intensity factor. Each output mode uses the reference's corresponding light or dark palette. Label and interface-tile seeds are transformed independently with the same equation, preserving their separate reference hues. The Jmol reference uses the original published RGB table before any contrast adjustments. There are no lightness offsets or additional coefficients. Lightness runs from 0–100%; intensity runs from 0–200%. The Presentation default uses 50% lightness and 200% intensity on light paper (L = 0.50, chroma × 2), and 70% lightness and 100% intensity on dark paper (L = 0.70, chroma × 1). Pastel retains its softer settings. Colors are mapped into sRGB, then automatic labels are adjusted for contrast. A color can reach the sRGB gamut limit before 200%, so increasing intensity further may not make it visibly stronger. Carbon and hydrogen remain neutral, including the H/count within NH and OH labels. Tiles derive their interaction tones from the generated seeds; existing ring fills are retained.

The **Save** icon adds or updates the theme in the local library and applies it with Undo. The **Export** icon writes both modes without changing the drawing. **Back to drawing** leaves unsaved changes unapplied. The optional `generator` field records the four settings, so a generated theme can be reopened and refined:

```json
"generator": {
  "light": { "lightness": 0.50, "chroma": 2.0 },
  "dark": { "lightness": 0.70, "chroma": 1.0 }
}
```

The exact formula is shown at the bottom of the generator:

```text
L = lightness / 100
C = reference C × intensity / 100
h = reference h
```

The optional `generator.reference` stores the reference's `id`, `name`, credits (`author`, `source`, `license`), complete `light`/`dark` element-color maps, and separate `light_tile_seeds`/`dark_tile_seeds` maps. Tile maps include inherited colors when the reference is captured. Older recipes without tile maps keep their existing behavior of deriving tiles from label seeds. These are frozen source colors, not a nested recipe or a live link to a library file. Reopening or exporting a generated theme preserves this snapshot even if the reference is later changed or removed. Missing `reference` means the original Jmol table, preserving earlier recipes. Ring fills continue to use the current drawing's colors.

Each number must be finite. `lightness` ranges from 0 to 1; `chroma` is an intensity multiplier ranging from 0 to 2. Explicit palette values remain authoritative when a theme is loaded: the recipe is applied only when a generation control changes. Existing files without a recipe remain supported. Earlier ReShiki builds that reject unknown fields or chroma factors above 1 cannot load those recipes; removing `generator` leaves a complete palette file for those builds.
