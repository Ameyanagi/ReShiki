# Roadmap

The goal is a complete molecular drawing workspace. These are implementation stages, not release commitments. Version 0.2 delivered clipboard editing, configurable/fused rings, transforms, multiline labels, arrow styles, templates, five-second recovery, standalone local packaging and PDF/PNG export. The remaining work below is broader than that iteration.

JACS / ACS is the required default for future drawing and export features. Build on the shared preset in `engine/drawing_style.json`; preserve physical sizes independently of screen zoom. Atom scripts and basic font measurements are now implemented, while rich user-editable labels and general collision avoidance remain work below.

1. **Editor completeness:** rich chemical labels, brackets, customizable templates, abbreviation expansion, attachment-aware fragment insertion, drawing styles and more precise label layout.
2. **Reliable documents:** native file association, open-file OS events, notarized release packaging, accessibility, full document-format validation and a larger interoperability corpus. Add exact text metrics and collision handling before claiming publication-quality rendering.
3. **Reaction workflows:** explicit reactant/product/reagent groups, reaction coordinates, atom mapping, stoichiometry, balancing and reaction file formats. Preserve semantics separately from arrow appearance.
4. **Advanced chemistry:** query atoms/bonds, R-groups, polymers/repeating units, enhanced stereo, multicenter attachments and advanced aromaticity cases. Add fixtures before enabling each new class.
5. **Publication and analysis:** build on physical page layouts, multipage PDF and native macOS printing with journal styles, configurable export styles, printing on other platforms, calculated properties and spectra integration. Chemical naming and NMR prediction need dedicated engines and separate correctness/licensing evaluation; RDKit alone does not deliver the whole scope.
6. **Pure Rust engine:** replace one operation at a time behind the existing contract. Start with formula/mass and graph validation; progress to parsing, aromaticity, stereo and canonicalization; tackle depiction independently. Keep differential tests against the current worker until each operation is proven on the supported corpus.

Acceptance should use real desktop workflows as well as backend tests. A menu entry, successful build, or one molecule round trip is insufficient evidence of complete capability.
