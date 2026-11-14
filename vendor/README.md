# Vendored crates

ReShiki builds these crates from local copies through `[patch.crates-io]` in the
root `Cargo.toml`. Each one is the published crates.io source with a small
change. The patch in `patches/` is the whole difference from that source,
generated against the crates.io package, whose sha256 the table records. Upstream's `Cargo.lock` is left out of
every copy, because ReShiki's own lockfile owns the dependency graph.

| Crate               | Version | Why ReShiki changes it                                                                                          | Patch                                                    | crates.io sha256                                                   |
| ------------------- | ------- | --------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------- | ------------------------------------------------------------------ |
| `iced_wgpu`         | 0.14.0  | Resize MSAA targets with the window, including when it shrinks.                                                 | [iced_wgpu.diff](patches/iced_wgpu.diff)                 | `ff144a999b0ca0f8a10257934500060240825c42e950ec0ebee9c8ae30561c13` |
| `iced_widget`       | 0.14.2  | Stop text carets blinking after 10 idle seconds, so an idle window stops redrawing.                             | [iced_widget.diff](patches/iced_widget.diff)             | `b1596afa0d3109c2618e8bc12bae6c11d3064df8f95c42dfce570397dbe957ab` |
| `iced_winit`        | 0.14.1  | Take a browser page's input method composition into Iced's text fields, and size the canvas under a strict CSP. | [iced_winit.diff](patches/iced_winit.diff)               | `7589888c8e951899cc688247a69933bb7a0511f0b4b2e122ac3fcd5dacb37f72` |
| `smithay-clipboard` | 0.7.3   | Share Iced's Wayland data device for bounded binary clipboard offers.                                           | [smithay-clipboard.diff](patches/smithay-clipboard.diff) | `71704c03f739f7745053bde45fa203a46c58d25bc5c4efba1d9a60e9dba81226` |
| `svg2pdf`           | 0.13.0  | Embed one instance per requested variable-font weight, so PDF text keeps its weight and stays selectable.       | [svg2pdf.diff](patches/svg2pdf.diff)                     | `e50dc062439cc1a396181059c80932a6e6bd731b130e674c597c0c8874b6df22` |
| `usvg`              | 0.45.1  | Carry the requested variable-font weight through shaping and glyph outlines.                                    | [usvg.diff](patches/usvg.diff)                           | `80be9b06fbae3b8b303400ab20778c80bbaf338f563afe567cf3c9eea17b47ef` |
| `window_clipboard`  | 0.5.1   | Give Iced's text fields the system clipboard in the browser.                                                    | [window_clipboard.diff](patches/window_clipboard.diff)   | `d5654226305eaf2dde8853fb482861d28e5dcecbbd40cb88e8393d94bb80d733` |
| `zune-jpeg`         | 0.5.15  | Upsample chroma and round the integer IDCT as libjpeg does.                                                     | [zune-jpeg.diff](patches/zune-jpeg.diff)                 | `27bc9d5b815bc103f142aa054f561d9187d191692ec7c2d1e2b4737f8dbd7296` |

Each crate also records its upstream repository, commit and the reason for each
changed file: `UPSTREAM.json` for the Iced crates, `smithay-clipboard` and
`window_clipboard`, `NOTICE-RESHIKI.md` for `usvg` and `svg2pdf`, and
`RESHIKI-PATCH.md` for `zune-jpeg`. All upstream sources are on crates.io, so
every patch is recovered in full.

## Check and update

Check that every vendored crate is still its upstream source plus its patch:

```sh
uv run --locked python scripts/check_vendor.py
```

The script reads each upstream source from Cargo's registry. When it is not
there, it downloads the crate file from crates.io and checks it against the
sha256 in the table. `cargo fetch` does not download these crates, because
`[patch.crates-io]` replaces them.

After you change a vendored crate on purpose, regenerate its patch and commit
both:

```sh
uv run --locked python scripts/check_vendor.py --write
```

To move a crate to a new upstream version, replace its directory with the new
registry source, apply its patch with `git apply -p1`, resolve any rejects,
update its `UPSTREAM.json` or notice and its version and sha256 in the table,
and regenerate the patch.

The root `.ignore` file hides `vendor/` from ripgrep-based searches. Git still
tracks it; search it with `rg --no-ignore`.
