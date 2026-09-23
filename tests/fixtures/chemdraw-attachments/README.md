These are our own synthetic interoperability drawings, opened, edited and saved
with ChemDraw 26.0.0.6599 on macOS using Computer Use on 2026-09-23.

- `eta3-allyl`, `eta6-arene`, `ferrocene`, `variable-arene`: generated with
  `examples/attachment_qa.rs`, moved in ChemDraw and saved. The CDX arene also
  records a view zoom change; its target IDs must survive the binary codec.
- `common-groups`: twelve generated N-substituted group examples. ChemDraw's
  Check Structure reported “No errors found.”
- `common-groups-expanded`: the same drawing after ChemDraw's Expand Label.

These files test representations and interchange, not synthesis, stability,
complete coordination spheres or oxidation-state validation. No proprietary
ChemDraw code or template library is included.

- `Cp-expanded`, `Cp-star-expanded`: expanded haptic ligand exports, edited and
  saved in ChemDraw; expect C10H10Fe and C20H30Fe respectively. Cp* passed Check
  Structure without errors after its ring orientation avoided the metal ray.
- `unsupported-haptic-abbreviation`: negative fixture. ChemDraw discarded the
  nested multi-center definitions in collapsed Cp* labels. Reject missing
  definitions; new editable exports expand haptic groups to avoid this loss.
- `hidden-dummy`: hidden unspecified wildcard with its two connected bonds.
