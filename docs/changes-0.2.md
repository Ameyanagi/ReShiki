# Moruno 0.2

This iteration expands everyday drawing and document handling. It does not complete the entire professional feature set.

## Editing

- Copy/cut/paste selected atoms, bonds, labels and arrows with ID remapping. Pasting plain SMILES, InChI, MOL or supported CDXML inserts a structure into the current drawing.
- Duplicate, rotate by 30 degrees, horizontal/vertical reflection, component alignment and distribution. Reflections preserve assigned stereochemistry by reversing wedge appearance. Transforming part of a connected molecule invalidates affected chemistry for rechecking.
- Three- through eight-membered rings, aromatic rings, shared-vertex placement and fused rings on an existing bond. Fused placement chooses the less occupied side.
- Wavy bonds, bond direction reversal, all 118 element symbols, and twelve insertable molecule templates.
- Forward, equilibrium, resonance, retrosynthesis and curved arrows; multiline labels and editing selected labels. `\n` in the label field inserts a line break.
- Double-click an atom to select its connected molecule. Labels stay upright during rotation/reflection.

Alignment moves connected selected atoms together; it does not collapse a molecule into a line. Distribution retains the overall selected span, so overlapping objects may need more room before distribution.

## Documents and output

- A recovery snapshot is saved every five seconds while there are changes. Startup offers drafts from interrupted sessions. Restore creates an unsaved copy and never overwrites the original source file. The most recent five seconds of work may not yet be in the draft.
- Save As and atomic replacement for native and export files. Native format version 2 retains arrow styles; version 1 documents still open and are upgraded when saved.
- Vector PDF and 1200 dpi PNG export from the same scene used for SVG. Drawing exports are cropped to the content; publication pages add standard/custom paper, margins, a page grid and multipage vector PDF.
- Basic CDXML text and forward arrows now import with their molecular drawing. Unsupported arrow styles and other drawing classes are rejected explicitly.
- A standalone Apple Silicon macOS bundle includes the Python runtime and RDKit. It remains a local development build signed ad hoc, without a notarized installer or Finder document association.

## Validation

Automated tests cover stereo-preserving copy/flip/rotation, all ring sizes, fused aromatic rings, clipboard references, group alignment, recovery persistence, worker recovery, native storage, and SVG/PDF/PNG generation. The fused aromatic test runs after the first ring is converted to Kekule bonds and verifies `C10H8`.

Desktop tests exercised fused naphthalene drawing and validation, clipboard paste, molecule movement, rotation/reflection/alignment, multiline annotation and equilibrium-arrow creation, and native saving. Additional rendering and recovery checks are recorded in the capability report.

Systematic naming, NMR prediction, semantic reaction mapping, polymers/query structures, full rich-text caret layout, advanced page composition, native printing and full accessibility remain substantial work. See `roadmap.md`; ruviz's likely role is documented in `ruviz-integration.md`.
