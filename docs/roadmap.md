# Roadmap

The goal is a complete molecular drawing workspace. These are implementation stages, not release commitments.

1. **Editor completeness:** configurable ring sizes, fused-ring placement, rotate/flip/align, object clipboard, bond direction reversal, full element selection, multi-line/rich chemical labels, brackets, curved and equilibrium arrows, fragment templates and label expansion.
2. **Reliable documents:** autosave/recovery, native file association, open-file OS events, standalone packaging, accessibility, full document-format validation and a larger interoperability corpus. Add exact text metrics and collision handling before claiming publication-quality rendering.
3. **Reaction workflows:** explicit reactant/product/reagent groups, reaction coordinates, atom mapping, stoichiometry, balancing and reaction file formats. Preserve semantics separately from arrow appearance.
4. **Advanced chemistry:** query atoms/bonds, R-groups, polymers/repeating units, enhanced stereo, multicenter attachments and advanced aromaticity cases. Add fixtures before enabling each new class.
5. **Publication and analysis:** page layouts, journal styles, PDF/raster exports, printing, calculated properties and spectra integration. Chemical naming and NMR prediction need dedicated engines and separate correctness/licensing evaluation; RDKit alone does not deliver the whole scope.
6. **Pure Rust engine:** replace one operation at a time behind the existing contract. Start with formula/mass and graph validation; progress to parsing, aromaticity, stereo and canonicalization; tackle depiction independently. Keep differential tests against the current worker until each operation is proven on the supported corpus.

Acceptance should use real desktop workflows as well as backend tests. A menu entry, successful build, or one molecule round trip is insufficient evidence of complete capability.
