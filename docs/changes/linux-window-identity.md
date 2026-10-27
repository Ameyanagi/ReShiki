# Linux window identity

[Issue #597](https://github.com/Ameyanagi/ReShiki/issues/597) reports missing window identity in GNOME.
Base: `1d0f720334f4ea7cdb246cc7e896f65e1a78591c`.
Validated patch SHA-256: `90257eb8efbfb716481c455d1e9c18896ef9962d969f3676dd349d6466bb2dbc`.

ReShiki now supplies `reshiki` as Iced's Linux application ID:

| Actual window metadata            | Before      | After                     |
| --------------------------------- | ----------- | ------------------------- |
| X11 `WM_CLASS` instance / class   | `""` / `""` | `"reshiki"` / `"reshiki"` |
| Wayland `xdg_toplevel.set_app_id` | `""`        | `"reshiki"`               |

Private Xvfb/Xlib and headless Weston probes read these values from real Linux x64 windows;
both applications remained running. The before control is the retained Nix v0.13.1 binary
from `b956e463` plus its recorded toolchain patch; its `src/main.rs` is byte-identical to the base.

The candidate's ordinary Rust/Cargo 1.99.1 Nix package build and original chemistry/PNG
install checks passed in **299.49 s**, reusing the store cache. No source toolchain was built.
Its installed `reshiki.desktop` has `Icon=reshiki`, `StartupWMClass=reshiki` and the matching PNG icon.

No GNOME dock/icon screenshot was taken. This verifies emitted window metadata and launcher
consistency; it adds no drawing-output comparison, GNOME interaction or performance guarantee.
