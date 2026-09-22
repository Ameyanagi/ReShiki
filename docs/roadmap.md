# Roadmap

ReShiki 0.6 includes native offline chemistry, direct drawing tools, selection-aware properties, reaction roles, templates, and an asynchronous assistant with editable previews and visual review. This roadmap describes remaining work, not release commitments. See [feature status](feature-status.md) for supported workflows and limitations.

JACS / ACS remains the default. Preserve physical drawing sizes, complete molecular graphs, and editable originals when adding features.

1. **Drawing quality:** improve exact label metrics and collision handling, rich-text caret layout, complex attachments, and orientation controls. Assistant corrections should be able to satisfy upright functional-group requests while preserving conventional bond angles.
2. **Document workflows:** improve native filename handling, recent files, OS open-document events, multiple documents, accessibility, and keyboard-only operation. Expand interchange checks across applications and Office versions.
3. **Reaction composition:** extend outward branches into general reaction networks, improve caption and condition placement, and add richer mapping, quantities, balancing and yield workflows. Keep reaction semantics separate from arrow appearance.
4. **Advanced chemistry:** add supported query atoms/bonds, R-group logic, polymer repeat units, enhanced stereo, multicenter attachments and wider aromatic cases only with independent fixtures and validation.
5. **Publication and analysis:** add reusable page/object stationery, calibrated actual-size view, broader printing controls and dedicated naming or spectra engines. Validate scientific correctness and licensing before exposing predictions.
6. **Native engine coverage:** extend Rust chemistry and the bundled official InChI kernel with independent differential comparisons. Python and uv remain development tools; installed applications must continue to work without them.

Acceptance includes real desktop workflows, saved and reopened drawings, Undo/Redo, and appropriate export checks. Follow the issues from the [0.6 release walkthrough](release-0.6-validation.md) for concrete UI, UX and performance findings.
