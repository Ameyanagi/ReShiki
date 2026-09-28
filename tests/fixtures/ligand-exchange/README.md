# Editable pi-ligand clipboard captures

Original test drawings captured on macOS on 2026-09-25 through an independent
desktop application's native clipboard. These are binary CDX objects, not
image wrappers. They contain no external templates or source code.

- `native.cdx`: user-created Fe–Cp reference, 7 nodes and 6 bonds, including
  five aromatic bonds and one five-member attachment target. No formal charge
  was assigned in this source drawing.
- `returned.cdx`: ReShiki's two-Cp drawing, pasted into the external editor,
  selected and copied back. It retains 13 nodes, 12 bonds, ten aromatic bonds,
  two five-member attachment targets, two hidden −1 charges and two closed
  12-point ellipse curves. Source: `docs/changes/fixtures/pi-ligands-copy.rsk`.
  The source metal charge is zero; this tests preservation, not a complete
  chemical assignment for a neutral complex.
- `gallery-returned.cdx`: the complete `assets/examples/shortcut-examples.rsk`
  document, copied through the normal editable clipboard pipeline, pasted with
  the copied drawing settings, selected and copied back. It contains 483
  nodes (including five attachment points), 429 bonds and 132 captions. Its
  three hidden Cp charges, Fe2+ charge, NO2/N3 group definitions and R/X text
  are retained. There are 33 aromatic bonds and one partial double bond.

The complete gallery capture inherited the old ReShiki CDX line-height unit
bug: its caption heights decode correctly to 0.4, 0.5, and 1.1 points, not the
8, 10, and 22 points the old decoder reported. Those paragraph values are
unsupported, so the complete original remains an expected rejection. A
separate diagnostic test excludes only the 132 standalone captions to verify
the original molecular objects, including all pi-ligand assertions. The fixture
bytes are unchanged; no importer fallback guesses the intended spacing.
Re-export `assets/examples/shortcut-examples.rsk` with the corrected build to
produce a CDX with properly encoded caption heights.
The [caption compatibility captures](../chemdraw-captions/README.md) verify a
complete actual clipboard return using the corrected outgoing encoder.

`tests/pi_ligand_exchange.rs` checks the captures and the current outgoing
writer, including topology, formal charges, attachment membership, aromatic
curves and absence of embedded pictures. A separate case moves a curve away
from its ring to ensure unrelated editable graphics are retained. R/X imports
are explicitly drawing-only; the strict query-chemistry parser still rejects
them. External round trips do not guarantee ReShiki's original 3D tilt metadata.
