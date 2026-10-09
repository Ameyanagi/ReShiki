# Chemical names

Open **Import → Chemical names…**. These tools use online services only after
you check the relevant consent box and press its request button. Consent is
cleared after each request. Name parsing sends the entered name to EMBL-EBI;
common-name and structure lookup send the name or molecular SMILES to NCBI
PubChem. No drawing file, image, caption or other document content is sent.
Normal drawing, preview editing, insertion and save/export remain native Rust;
the installed application needs no Java or Python runtime.

## Name to editable structure

Choose **Systematic name · OPSIN online** for nomenclature covered by the
[OPSIN parser](https://www.ebi.ac.uk/opsin/). Choose **Common name · PubChem
online** for an exact name or synonym present in that database. A successful
parse or single database hit is a source interpretation, not proof that an
informal alias has only one meaning. Review the resulting graph and specified
stereochemistry before inserting it.

When a source returns multiple database records, choose an interpretation;
none is inserted automatically. OPSIN ambiguity/stereochemistry warnings
remain visible and require acknowledgment before insertion. Unsupported
names, incomplete service responses, unsupported chemical graphs and failed
connections produce an explanation and leave the drawing unchanged.

The preview is a native editable molecular graph. Drag atoms to adjust its
layout. Edit its SMILES and press **Update preview** to change chemistry
locally, or **Restore source** to return to the source interpretation. A
changed chemical identity explicitly invalidates applicability of the
original source name. Unapplied SMILES edits cannot be inserted. The graph is
verified again before insertion; coordinate edits cannot silently substitute
a different stereoisomer. **Insert editable structure** creates one Undo
step. The inserted molecule supports normal editing, native save and chemical
export.

## Structure to source names

Select one complete connected molecule, check consent for sending molecular
SMILES, and press **Look up selected structure online**. The selected graph
includes all underlying atoms in an abbreviation. A selection that cuts a
bond is rejected rather than being treated as a new compound.

The service uses [PubChem PUG REST](https://pubchem.ncbi.nlm.nih.gov/docs/pug-rest)
identity search with `same_stereo_isotope`. ReShiki then parses each returned
isomeric SMILES locally and compares canonical graph identity, including
connectivity, charge, isotope, tautomer and specified tetrahedral/double-bond
stereo. PubChem standardization to a different graph or unresolved multiple
matches is rejected. Map numbers and drawing atom order are ignored.

Results distinguish the **systematic name from PubChem lookup** from the source
title and source-supplied synonyms. Synonyms can include registry identifiers;
they are not all endorsed common names. The CID and **Source** link retain
provenance. **Copy name** copies the systematic name; **Insert caption** adds
an undoable caption only while the named graph is still current and selected.

This stage retrieves a source's systematic name. It does not claim to
generate a general IUPAC name locally. Novel structures and structures absent
from PubChem may have no result.

## Supported chemical domain

Both directions accept ordinary connected molecular graphs with 1–512 atoms,
native-supported elements, ordinary single/double/triple/aromatic bonds,
charges, isotopes and specified native-supported tetrahedral/E/Z stereo.
Unspecified stereo remains unspecified; lookup cannot add an absolute
configuration. Wavy/unknown or unresolved stereo, relative stereo groups,
queries, radicals, unusual bond orders, multi-center/variable attachments,
mixtures and disconnected salts are not supported by this workflow. Ordinary
native drawing/import support outside this naming domain remains separate.

Requests have a 20-second HTTP timeout, a 1 MiB response limit, at most 16
candidate records and no automatic retries. Starts across tabs are throttled
to at most two per second. Only 12 bounded source synonyms are displayed.
There are no background naming requests, bulk downloads or paid APIs.

## Engines, licenses and deployment

The [official OPSIN repository](https://github.com/dan2097/opsin) describes an
MIT-licensed Java parser with broad organic systematic nomenclature support.
Its local distribution requires Java 8 or later. ReShiki uses the documented
[EMBL-EBI JSON web service](https://www.ebi.ac.uk/opsin/#web-service) to keep
the installed runtime a Rust executable; no OPSIN code or Java runtime is
bundled. Parser status, message and warnings are preserved. Service availability
and coverage can change independently of the application.

PubChem is a source database rather than a local naming engine. Its
[download guidance](https://pubchem.ncbi.nlm.nih.gov/docs/downloads) explains
that contributor-specific licensing conditions can apply. ReShiki does not
bundle or redistribute a synonym database; requested results carry their CID
and record link. No third-party engine source code is copied into this feature.

## Validation and reproduction

The retained pre-feature control `tests/chemical_naming_baseline.rs` records
that importing `ethanol` as SMILES fails while `CCO` produces the expected
editable ethanol graph. Graph fixtures cover ethanol, aspirin, both lactic
acid enantiomers, E/Z but-2-ene, unspecified stereo, carbon-13 ethanol and
acetate. Focused tests check exact identity, unsupported graphs, incomplete
source results, synthetic multi-record ambiguity, parser warning preservation,
native preview insertion/Undo/Redo/save, independent consent and stale async
results. The optional live check uses public chemical fixtures only:

```sh
cargo test -p reshiki --lib naming::tests
cargo test -p reshiki --bin reshiki app::naming::tests
cargo test -p reshiki --lib live_official_services_resolve_names_and_exact_stereoisomers -- --ignored
```

Desktop examples: parse `ethanol`, edit the preview layout and insert it;
select the resulting molecule and look up its source name/synonyms. Parse
`(R)-lactic acid` and compare its identity with `(S)-lactic acid`; use
`(+)-lactic acid` to inspect optical-rotation warnings. Enter
`reshiki-no-such-chemical-name-50` for unsupported input. No online request
should begin while its corresponding consent is unchecked.
