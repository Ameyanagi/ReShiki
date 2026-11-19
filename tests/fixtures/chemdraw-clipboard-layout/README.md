# Clipboard layout preservation

These synthetic inputs deliberately use unequal bond lengths, oblique chains,
asymmetric molecule spacing, a slanted reaction arrow, two captions, a plus,
and a rectangle. They are independently authored test data, not native ChemDraw
captures or ReShiki exports. `generate.py` uses the retained Python reference
codec only for CDX/CDS framing. Its local native arrow tag correction follows
the provenance in `chemdraw-import-corpus/manifest.json`.

`valid-layout`, `explicit-30`, and `missing-bond-length` have identical paper
geometry, with root BondLength 14.4, 30, and absent respectively. An absent
BondLength has the drawing-style default 30pt, already implemented in
`presentation/style.rs`; it must also be used in the detached molecular parsing
copy. The original source and public low-level native reader remain unchanged.

The preservation contract allows the existing paper/world conversion 42/14.4
and a single common placement translation. Bond ratios, angles, molecule gaps,
and stamp positions must remain unchanged. Independent component scaling,
rotation, or coordinate regeneration would violate the contract.
Editable export may represent the two stamps as three paths. The regression
checks their six actual straight edges across that representation change,
in addition to the source frames on initial import.

`unsupported-graphic` adds a valid gradient-shaded p orbital, a style ReShiki
cannot draw yet. The drawing imports whole, the orbital in flat grey, and the
import says so. `misleading.smiles` describes the same molecular identities but
loses the deliberate geometry and reaction stamps; it must not silently replace
failed editable drawing clipboard data.

CDS variants add native packed LabelStyle/CaptionStyle records to the complete
CDX artwork. All inputs and their expected contracts are retained, including
the missing-default inputs that initially failed bond association.

References: [CDXML DTD](https://static.chemistry.revvitycloud.com/cdxml/CDXML.dtd),
[CDX rectangle styles](https://iupac.github.io/IUPAC-FAIRSpec/cdx_sdk/properties/RectangleType.htm),
and the repository's `reference/engine/cdx_exchange.py`.
