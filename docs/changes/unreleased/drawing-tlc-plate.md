---
section: Changes
issues: []
---

- ChemDraw TLC plates open as one editable drawing object with lanes, filled or
  hollow spots, origin and solvent-front lines, physical spot sizes and styled
  Rf labels. Native ReShiki files and editable CDXML/CDX keep the plate data;
  canvas, SVG and PDF use its shared geometry. Unsupported custom spot artwork,
  tails and appearance settings produce an import notice.
- Native TLC exchange keeps ChemDraw's five-point lane margins and authored Rf
  captions, and writes binary CDX show flags in the form ChemDraw can reopen.
  Out-of-range Rf text sizes use the normal caption fallback while retaining
  authored text, styling and placement.
