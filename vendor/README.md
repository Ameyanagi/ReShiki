# Vendored crates

ReShiki builds these crates from local copies through `[patch.crates-io]` in the
root `Cargo.toml`. Each one is the published crates.io source with a small
change. The patch in `patches/` is the whole difference from that source,
generated against Cargo's registry copy. Upstream's `Cargo.lock` is left out of
every copy, because ReShiki's own lockfile owns the dependency graph.

| Crate               | Version | Why ReShiki changes it                                                                                          | Patch                                                    |
| ------------------- | ------- | --------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------- |
| `iced_wgpu`         | 0.14.0  | Resize MSAA targets with the window, including when it shrinks.                                                 | [iced_wgpu.diff](patches/iced_wgpu.diff)                 |
| `iced_widget`       | 0.14.2  | Stop text carets blinking after 10 idle seconds, so an idle window stops redrawing.                             | [iced_widget.diff](patches/iced_widget.diff)             |
| `iced_winit`        | 0.14.1  | Take a browser page's input method composition into Iced's text fields, and size the canvas under a strict CSP. | [iced_winit.diff](patches/iced_winit.diff)               |
| `smithay-clipboard` | 0.7.3   | Share Iced's Wayland data device for bounded binary clipboard offers.                                           | [smithay-clipboard.diff](patches/smithay-clipboard.diff) |
| `svg2pdf`           | 0.13.0  | Embed one instance per requested variable-font weight, so PDF text keeps its weight and stays selectable.       | [svg2pdf.diff](patches/svg2pdf.diff)                     |
| `usvg`              | 0.45.1  | Carry the requested variable-font weight through shaping and glyph outlines.                                    | [usvg.diff](patches/usvg.diff)                           |
| `window_clipboard`  | 0.5.1   | Give Iced's text fields the system clipboard in the browser.                                                    | [window_clipboard.diff](patches/window_clipboard.diff)   |
| `zune-jpeg`         | 0.5.15  | Upsample chroma and round the integer IDCT as libjpeg does.                                                     | [zune-jpeg.diff](patches/zune-jpeg.diff)                 |

Each crate also records its upstream repository, commit and the reason for each
changed file: `UPSTREAM.json` for the Iced crates, `smithay-clipboard` and
`window_clipboard`, `NOTICE-RESHIKI.md` for `usvg` and `svg2pdf`, and
`RESHIKI-PATCH.md` for `zune-jpeg`. All upstream sources are on crates.io, so
every patch is recovered in full.

## Check and update

Check that every vendored crate is still its upstream source plus its patch:

```sh
cargo fetch --locked
uv run --locked python scripts/check_vendor.py
```

After you change a vendored crate on purpose, regenerate its patch and commit
both:

```sh
uv run --locked python scripts/check_vendor.py --write
```

To move a crate to a new upstream version, replace its directory with the new
registry source, apply its patch with `git apply -p1`, resolve any rejects,
update its `UPSTREAM.json` or notice, and regenerate the patch.

The root `.ignore` file hides `vendor/` from ripgrep-based searches. Git still
tracks it; search it with `rg --no-ignore`.
