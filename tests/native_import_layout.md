# Native import layout completion

This checkpoint completes `native_import::execute` for SMILES, InChI, CDXML/CDX
without a conformer, and reaction SMILES participants without a conformer. It
uses the detached public solver from `00b8f1d`; it does not edit engine routing
or the Python reference worker.

SMILES always replaces CX coordinates after their chemical interpretation.
InChI runs the bounded asynchronous native reader, reconstructs with
`sanitize=true, remove_hydrogens=false`, checks supported chemistry, and lays
out before drawing construction. CDXML scene assembly precedes missing-layout
completion. Reaction imports lay out pending participants in their existing
reactant/agent/product order and analyze the finished combined drawing. All
these original calls use `useRingTemplates=false`. Analysis retains original
molecular chemistry and full f64 coordinates; final drawings use the existing
actual `Value -> Response` transport semantics.

Raw CX rank strings survive hydrogen removal by original kept-atom index.
`depict::compute` retains its numeric API. The additional
`compute_with_rank_properties` accepts absent, numeric and invalid property
values. Native-generated CIP ranks take precedence; otherwise `_CIPRank`
precedes `_chiralAtomRank`. Invalid conversion is raised only when the native
ordering reads that atom. For example, `CC |atomProp:0._CIPRank.bad|` succeeds,
while atom 1 fails; every rank can remain unread in a fully seeded cyclohexane.
The additional rank passes consume the solver's shared work allowance.

The conversion follows pinned RDKit revision
`0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985`, wheel `2026.03.6`:

| Source                                           | Adapted behavior                                                                  | SHA-256                                                            |
| ------------------------------------------------ | --------------------------------------------------------------------------------- | ------------------------------------------------------------------ |
| `Code/GraphMol/Depictor/DepictUtils.cpp:578-612` | Selected-atom property reads, CIP precedence, unsigned fallback and pair ordering | `ba3868f339635889b1533fc6a706a5a1d8f6739846e1e25e66e7efa0617eddd5` |
| `Code/RDGeneral/RDValue.h:271-296`               | Right trimming and lazy unsigned string conversion                                | `0a7eaee5302eaf4b4f5a131650becc84b17a4af3e60693279abca22753814c69` |

RDKit's BSD notice includes RDValue.h. The conversion oracle retains the
platform exception spelling separately: libc++ reports `bad any cast`, while
libstdc++ reports `bad any_cast`. Rust exposes typed `BadRank` failures; this
message difference does not broaden accepted inputs or numerical comparisons.

Linux x86_64 Rust 1.95.0 validation (four build jobs, isolated target directory):

- All seven `native_import` tests pass. Independent original worker corpora
  compare 6,301 complete responses and 2,545 matching failures. Four existing
  CDXML contract restrictions remain separately counted. The newly added
  717-case layout corpus contributes 486 successes and 231 failures, with no
  new restriction. Responses are compared after `Value -> Response`, with only
  the existing four descriptor relative tolerances (`1e-12`); drawing numbers,
  identities, styles and all other fields compare exactly.
- `depict_rank_properties` covers 1,344 direct public native calls: 1,232
  successes, 112 lazy conversion failures, and 15,096 exact f64 coordinate
  scalars. Source snapshots remain unchanged; dimension and shared-budget
  failures are checked separately.
- The complete public pipeline also passes all 2,854 cases (2,758 successes,
  96 rejections, 130,722 exact coordinate scalars).
- Existing attachment, seed, template and expansion suites pass unchanged,
  including strict Linux native geometry. Cross-platform fixture audits remain
  separate and are not evidence of host parity.
- The library tests pass with the single pre-existing
  `native_layout_deferrals_preserve_the_original_request` test excluded: it still expects the old worker deferral; the engine/reference-feature owner is replacing it
  during integration. The import implementation now completes that request.

The request tests also cover atomic layout failure, InChI reader heap failure,
recovery on the same immutable request, and a native null parse result. No
partial document is published. macOS and Windows replay belongs to parent
integration; no local Cargo run was used on those hosts.
