# ReShiki 0.5.0

ReShiki 0.5.0 adds native Windows clipboard and printing support, editable drawings in Microsoft Office, and more chemistry and image processing in Rust.

- **Editable Office drawings on Windows:** copy from ReShiki into desktop Word, PowerPoint or Excel, double-click to edit, and press Ctrl+S to update the embedded drawing. Transparent vector previews stay sharp when resized. In Excel, set the object's fill to **No fill** to show worksheet cells through it.
- **Windows clipboard and printing:** native Copy/Paste preserves editable drawings, Copy Image offers static image formats, and the Windows print dialog supports page ranges and printer preferences. The installer registers ReShiki as the embedded drawing editor.
- **Windows rendering:** direct software rendering avoids expensive GPU emulation on virtual machines and remote desktops. Clipboard, printing and process support are built into the application without a .NET runtime or extra helper.
- **Rust chemistry and file handling:** binary CDX conversion, molecular formulas and masses, valence checks, hydrogen counts, and bounded ring perception now run in Rust. Ring calculations retain a checked RDKit fallback when agreement cannot be established.
- **Images:** embedded raster normalization runs in Rust, preserving orientation, reflection and transparency. Pillow is no longer installed in the production chemistry environment.
- **Documentation:** the [Windows guide](/guide/windows/) covers installation, native printing and verified Word, PowerPoint and Excel editing workflows.

Install [uv](https://docs.astral.sh/uv/getting-started/installation/) before first launch. Python and RDKit are still required and are set up automatically. Downloads cover Apple Silicon macOS, Windows x64/ARM64, and Linux x64/ARM64. macOS packages are signed and notarized; Windows and Linux packages remain unsigned.

Office double-click editing requires desktop Office on Windows. Keep a separate `.reshiki` original when sharing editable drawings with a Mac. See the [Windows notes](windows.md) for platform limits and the [architecture notes](architecture.md) for the Rust migration's verification scope.
