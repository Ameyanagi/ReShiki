# Coordination projection fixture

`co-en3-external-source.cdxml` is an original reference fixture authored for issue
246, using CDXML properties understood by ChemDraw. It has two Co(en)3 diagrams,
each with three N–C–C–N ligands, six explicit N→Co dative contacts and Co charge
+3. The left uses donor-narrowed `WedgeBegin`/`WedgedHashBegin`; the right uses
metal-narrowed `WedgeEnd`/`WedgedHashEnd`. Neither assigns Δ/Λ stereo. This is
original test data, not a file recorded or re-saved by ChemDraw. Root's external
application review records actual ChemDraw interpretation separately.

ChemDraw 26.0.0.6599 opened both diagrams as six donor→Co arrow bonds each,
retained NH2 and Co3+, and ignored the wedge/hash appearance. Save As retained
`Order="dative"` and donor/metal B/E, adding `BS="N"`, but removed all Display
attributes. This fixture proves the chemical graph/direction convention;
ReShiki's projection paint needs separate native/figure evidence and has an
explicit ChemDraw interchange limitation.

`co-en3-chemdraw-roundtrip.cdxml` is that actual Save As output from the
original fixture, recorded on 2026-10-09. Its regression checks both intact
complexes and the retained donor hydrogens/direction, and explicitly expects
the removed projection paint.


`co-en3-before.rsk` contains the intact three-ligand skeleton and Co3+ before
coordination (13 atoms, 9 bonds). `co-en3-after.rsk` preserves every atom, position
and cached NH2 label and adds six N→Co contacts. The first pair has solid wedges,
the next pair has tapered hashes, with one donor-narrowed and one acceptor-narrowed
contact in each pair; the final pair uses ordinary dative arrows. These appearances
are drawing projections and do not assign complex stereochemistry.

`co-en3-both-tips.{rsk,cdxml,cdx,svg}` are generated from the same complete graph
by the IO regression. The native file also refreshes carbon H labels from the
complete chemistry. CDXML/CDX preserve direction and style for ReShiki; the
actual ChemDraw limitation above still applies. SVG retains the native projection
paint. Regenerate only with an explicit `RESHIKI_COORDINATION_FIXTURES` directory
while running the model and IO coordination suites.
