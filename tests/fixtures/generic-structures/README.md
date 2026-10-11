These are small source fixtures for typed generic import and expansion tests.

`anonymous-mixed.cdxml` is inferred from the ChemDraw CDX SDK anonymous-node and external-connection rules and native abbreviation port topology. It is not a ChemDraw 26 export receipt. It supplies explicit one-port methoxy and phenoxy fragments; the reader verifies their chemistry before accepting their labels.

`link-chain.cdxml` is a hand-authored, explicit two-connection link node with counts 1–3. Producer comparison in ChemDraw remains a separate native UI check.

`variable-arene-native-refused-v26.rsk` records a benzene variable-attachment site and an explicit methoxy branch. Its six attachment positions describe one unique anisole product. It also reproduces the old ring-crossing attachment layout.

`variable-methoxytoluene-v26.rsk` adds an outward methyl group at atom 1 and limits the variable site to atoms 2–6. Expansion should retain three chemically distinct methoxytoluene products: ortho, meta and para. Both native inputs use document format 26; the latter is a hand-authored derivative of the former, not a reference-application export.

`chemdraw26-anonymous-ome-oph.cdxml` and `chemdraw26-anonymous-ome-oph-expanded.cdxml` are immutable ChemDraw 26.0.0.6599 saves of the two-atom source and the two products produced by Structure > Expand Generic Structure. Their exact producer actions and SHA-256 values are recorded in `chemdraw26-anonymous-producer-receipt.json`. Tests compare the concrete Rust products with the native expanded products by canonical molecular graph identity.
