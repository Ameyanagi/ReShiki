# Chemical names

Chemical names turns a name into an editable structure and gives a molecule its
systematic name. The default parser runs on your computer without an internet
connection or Java. An optional PubChem lookup can resolve common names outside
the local parser's vocabulary, after you confirm that the name may be sent.

## Use it

### Insert a structure from a name

1. Open **Import**, choose **Name** and type a name such as `ethanol`.
2. Choose **Insert**. **Add name below** is on at first and adds the name as a
   caption below the structure; turn it off to insert only the structure. Undo
   removes both, and Redo restores both.
3. Optional: choose **Preview** to check the structure first. Drag atoms to
   arrange the layout, or edit the **SMILES** and choose **Apply**. **Reset**
   restores the parsed structure.

**Cancel** stops a pending import. Editing the name or switching back to
**Structure** cancels it too. If you change the chemistry in the preview, the
typed name no longer matches, so ReShiki inserts the structure without it.
Apply SMILES edits before you insert.

### Look up a common name online

1. Enter the complete name under **Import → Name**. If local parsing cannot
   resolve it, choose **Look up online…**.
2. Review the name and provider in **Send this name to PubChem?**. Choose
   **Send name** to contact `pubchem.ncbi.nlm.nih.gov` (NCBI), or **Cancel** to
   stay offline. This sends the entered name, not your drawing. Every lookup
   requires a new confirmation; local parsing never starts an online request.
3. Review the returned structure and PubChem CID in the preview, then choose
   **Insert**. Lookup alone does not change the drawing or Undo history.

Changing the name or choosing **Cancel** stops a pending lookup. Multiple
compound matches are rejected rather than choosing one silently. Salt and
mixture results retain their components and are rejected by the existing
single-molecule import limit; they are not converted to a free base.
If PubChem is unavailable, local parsing and SMILES import remain available.
The same confirmation flow is available in the Chemical names inspector.

### Show a molecule's name

1. Right-click a molecule and choose **Show chemical name**. The name appears
   centered below the molecule; for ethanol it is `ethanol`.
2. Choose **Hide chemical name** in the same menu to remove it.

Show and Hide are each one Undo step. The clicked molecule is named even when
other molecules are selected or grouped with it. The caption moves with the
molecule. When the chemistry changes, the caption is removed in the same Undo
step. Editing or dragging the caption turns it into ordinary text that stays.

To see or copy a name without placing it, select one whole molecule and choose
**Generate selected name** in **Import → Name → Details** or in the
[Chemical names inspector](#chemical-names-inspector). Then choose **Copy name**
or **Insert caption**.

### Curated offline aliases

1. In **Import → Name**, type a name and open **▸ Details**. If Details is
   already open, choose **Resolve name**.
2. **CURATED ALIASES · OFFLINE** lists other names for the same structure, each
   with its source and license.
3. Choose **Copy** to copy an alias, or **Use below** to use it as the caption of
   the next insertion. **Use original name below** switches back to the typed
   name.
4. To name a drawn molecule, select it and choose **Generate selected name** in
   the same Details area. **GENERATED SYSTEMATIC NAME · OPENCLATURA** shows the
   result with **Copy name** and **Insert caption**.

Opening Details does not open the preview or insert anything. Changing the
typed name clears the alias choice. An alias caption replaces the molecule's
existing name caption instead of adding a second one.

### Chemical names inspector

Right-click a molecule and choose **Chemical names…**. ReShiki selects that
molecule and opens the **Chemical names** inspector. It has two parts:

- **NAME → STRUCTURE**: enter a name and choose **Parse name**. Drag atoms in
  the preview to arrange the layout, or edit its SMILES and choose **Update**;
  **Restore** returns to the parsed structure. If the result has warnings,
  check **I reviewed these warnings and the structure**. Choose **Insert** to
  add the editable structure. It is inserted without a name caption.
- **STRUCTURE → NAME**: choose **Generate selected name**, then **Copy name** or
  **Insert caption**. A tag under the generated name gives its
  [preferred IUPAC name status](#preferred-iupac-name-status). Curated aliases
  for the molecule appear below the generated name.

**‹ Properties** returns to the Properties inspector, and **Import** opens the
Import tab.

## Limits

- Ambiguous names, unsupported or ignored stereo, relative or racemic
  qualifiers, radicals, polymers, queries and disconnected salts are rejected
  with a reason. A rejected name leaves the drawing and Undo history unchanged.
  A salt or mixture name, such as `sodium chloride`, is summarized as
  "Salts and mixtures aren't supported yet. Draw the ions, or paste SMILES
  under Import → Structure." rather than as an ambiguous name.
- Optical rotation signs are not turned into R/S. The one exception is
  `(+)-lactic acid`, which reads as `(2S)-2-hydroxypropanoic acid`.
- Generated names cover one connected molecule: chains, rings, ring
  assemblies, spiro, bridged and fused ring systems, with charges and R/S and
  E/Z stereo. Salts and mixtures, isotope-labeled molecules and a few stereo,
  charge and ring states that the name could misdescribe give an error
  instead of a name.
- Generated names are systematic. They are not always the preferred IUPAC name.
- The alias catalog matches the exact structure, so stereo, charge and
  isotopes count. **No curated aliases available in the bundled catalog.** only
  means the catalog has no entry, not that the molecule has no other names.
- Other formats keep a caption's text but not its link to the molecule.

## Implementation notes

### Name to structure

The Rust OPSIN port handles supported systematic and retained names, including
substituted functional groups, bicyclic/spiro systems, heterocycles, supported
absolute R/S and E/Z stereo, isotopes and formal charge. The editor accepts a
connected graph with at most 512 atoms. ReShiki verifies the editable graph
before inserting it. The specific retained name `(+)-lactic acid` is normalized
locally to `(2S)-2-hydroxypropanoic acid`, using
[ChEBI:422](https://www.ebi.ac.uk/chebi/CHEBI:422), then parsed and checked
against its explicit S graph. The parser result identifies this retained-name
source. Other optical-rotation names, including derivatives, salts and
conflicting descriptors, remain subject to strict rejection; this entry is not
a general sign-to-configuration rule. The original input can be used for **Add
name below**, while structure-to-name returns the verified
`(2S)-2-hydroxypropanoic acid`. Older screenshots that show this name rejected
predate this change. Changing the input or drawing invalidates pending or
outdated results.

### Structure to name

Abbreviated groups are expanded for naming. Showing an already named molecule
reuses its caption. Names come from a Rust port of
[openclatura](https://github.com/lamalab-org/openclatura) 0.4.3 by lamalab-org
(MIT license; commit `90dc5bcb`), in `src/naming/openclatura/`. The port follows
the Python modules and rule order, and its JSON rule tables in
`src/naming/openclatura/data/` are byte-identical to upstream. RDKit's part is
done by `crates/chemistry`: SMILES parsing, kekulization, ring perception,
stereo perception and the CIP labeler, including r/s. The copyright and license
are in `licenses/openclatura/` and `NOTICE`.

The port names one connected molecule of up to 512 atoms: chains, monocycles,
heterocycles, ring assemblies, spiro systems, von Baeyer (bicyclo, tricyclo)
systems and fused ring systems, including retained names such as `naphthalene`
or `1H-indole`, fusion names such as `pyrazolo[3,4-d]pyrimidine`, hydro
prefixes, indicated hydrogen and bridged fused parents. It covers functional
groups, charges and ions, and R/S, r/s and E/Z. It declines salts and mixtures,
isotope labels, and the cases where upstream's own name would describe another
structure: nitrogen and ring lone-pair stereocentres, some ring stereo, r/s on
von Baeyer parents, and some charge, hydrogen and localized pi states. Each
such place is marked `Upstream divergence` in the source. The text matches
openclatura 0.4.3 exactly for 98.6% of the test corpus; the
[structure-to-name harness](structure-to-name-harness.md) has the numbers.

The worker generates the name, a second worker parses it with the OPSIN Rust
port, and the name appears only when that parse gives the same graph, charge
and specified stereo. Otherwise you see an error and no name. Generation runs
under the same worker limits as parsing, so a very large ring system can time
out. Comprehensive IUPAC coverage is not claimed. Unspecified stereo remains
unspecified.

### Preferred IUPAC name status

Every generated name carries a status that says how it relates to the
preferred IUPAC name (PIN) of the IUPAC 2013 recommendations. The Names
inspector shows it as a tag under the name, with the reason on hover. Copy
name and Insert caption use the name text alone.

| Tag                                           | Meaning                                                                                       |
| --------------------------------------------- | --------------------------------------------------------------------------------------------- |
| Preferred IUPAC name                          | No rule on the name's path is known to depart from the PIN rules.                             |
| Systematic name, not the preferred IUPAC name | The name is correct but uses a form the PIN rules do not, such as `methanal`.                 |
| Systematic name, preference not verified      | The name took a path whose PIN choice has not been checked, such as an OPSIN-numbered fusion. |

The engine marks the status where it makes the choice, so the name text is
unchanged. A naming attempt it discards leaves no mark. Reasons for a name
that is not the PIN:

| Reason                    | Example the engine writes                                                  | Preferred form                                                |
| ------------------------- | -------------------------------------------------------------------------- | ------------------------------------------------------------- |
| `sulfonyl_prefix`         | `N-methyltosylamino`, `(methylsulfinyl)amino`                              | `N,4-dimethylbenzene-1-sulfonamido`, `methanesulfinamido`     |
| `amino_prefix`            | `3-methylguanidino`                                                        | `(N-methylcarbamimidoyl)amino`                                |
| `functional_class`        | `dimethyl sulfoxide`, `acetaldehyde hydrazone`, `trimethylphosphane oxide` | substitutive names                                            |
| `hydride_parent`          | `dimethylsulfane`, `trimethylazane`                                        | `(methylsulfanyl)methane`, `N,N-dimethylmethanamine`          |
| `unretained_parent`       | `methanal`, `ethanediamide`, `methoxybenzene`, `hydroxyformyl chloride`    | `formaldehyde`, `oxamide`, `anisole`, `carbonochloridic acid` |
| `fusion_prefix`           | `oxazolo[4,5-b]pyridine`, `isoxazolo`                                      | `[1,3]oxazolo[4,5-b]pyridine`, `[1,2]oxazolo`                 |
| `diamide_locants`         | `N,N'-diethylpropanediamide`                                               | `N1,N3-diethylpropanediamide`                                 |
| `multiple_bond_numbering` | `pent-4-en-1-yne`                                                          | `pent-1-en-4-yne`                                             |

Reasons for an unverified name: `opsin_fusion_layout` (fusion numbering chosen
for OPSIN compatibility), `bridged_fused_parent`, `unaudited_parent`
(porphyrin, corrin, gonane), `substituted_retained_parent` (substituted
acetaldehyde), `atom_order` (bridged fused ketones whose added hydrogen depends
on input atom order), `amine_chalcogenide` (amine sulfide and similar names) and
`principal_group_tie` (equally senior principal groups, such as the chain and
ring acids of `2-(4-carboxyphenyl)acetic acid`, chosen by atom order rather than
by P-44.1). The other parent-selection rules that the port does not implement in
full (heteroatom seniority, skeletal replacement chains, phane names) have no
mark.

Sulfonyl and sulfinyl prefixes are named from the acid (P-65.3.2.2.2) and
enclosed although simple (P-16.5.1.4): `(methanesulfonyl)benzene`,
`4-(4-methylpiperazine-1-sulfonyl)benzoic acid`,
`3-[(methanesulfonyl)oxy]propanoic acid`, `N-(sulfooxy)cyclohexanimine`. The
`sulfonyl_prefix` mark stays where the engine still concatenates: on a nitrogen
that the PIN names as sulfonamido or sulfinamido (P-66.1.1.4.3), on an imino
nitrogen, and where another acid group in the substituent would take the acid
suffix.

Chalcogen analogues of oxo and formyl take the preferred prefixes
`sulfanylidene`, `selanylidene` and `tellanylidene` (P-64.6.1), multiplied with
`bis` (P-16.3.6), and `methanethioyl`, `methaneselenoyl` and `methanetelluroyl`
(P-66.6.3): `3-sulfanylidenebutanoic acid`,
`3,5-bis(sulfanylidene)hexanoic acid`, `4-(methanethioyl)benzoic acid`.

Alkoxy prefixes use the retained contractions `methoxy`, `ethoxy`, `propoxy`,
`butoxy`, `phenoxy` and `tert-butoxy` (P-63.2.2.2), not isopropoxy, sec-butoxy
or isobutoxy: `[(propan-2-yl)oxy]benzene`, `[(butan-2-yl)oxy]benzene`
(P-63.2.2.1.1), `(2-methylpropoxy)benzene`.

Amino prefixes use the retained `anilino` for C6H5-NH-, with full substitution
(P-62.2.1.1.1), and `carbamimidoylamino` for guanidine as a prefix
(P-66.4.1.2.1.3): `2-anilinoacetic acid`, `4-(N-phenylanilino)benzoic acid`,
`2-(carbamimidoylamino)acetic acid`, `2-[carbamimidoyl(methyl)amino]acetic acid`.
Contractions keep phenyl: `methyl phenylcarbamate`, `(phenylsulfamoyl)`.

Chains take systematic names where the retained names are only for general
nomenclature (P-65.1.1.2.2, P-66.5.1.2.3): `propanenitrile`, `butanenitrile`,
`propanedioic acid`, `butanedioic acid`, `pentanedioic acid` and
`hexanedioic acid`, not propionitrile, butyronitrile, malonic, succinic,
glutaric or adipic acid. HCN is `formonitrile` (P-66.5.1.2.1).

`unretained_parent` also covers formic acids, formyl halides and formamides
substituted on carbon, which are carbamic or carbonic acid derivatives
(P-65.1.8.1). Substituted anisoles such as `1-chloro-4-methoxybenzene` are
preferred names (P-63.2.3).

Enclosing marks nest in the P-16.5.4 order `{[({[( )]})]}`:
`2-[4-(2-methylpropyl)phenyl]propanoic acid`,
`2-(2-{2-[2-(2-hydroxyethoxy)ethoxy]ethoxy}ethoxy)ethan-1-ol`. Brackets that
belong to the parent name are ignored when choosing the level: fusion, von
Baeyer and spiro descriptors (`2-(bicyclo[2.2.1]heptan-2-yl)acetic acid`) and
the parentheses of indicated or added hydrogen, such as `1(2H)`.
Stereodescriptors and compound locants count as a level
(`4-[(2E)-but-2-en-1-yl]phenol`), and a mark that would sit next to one of the
same kind moves up a level (P-16.5.4.1.5). The nesting does not change the
status.

`reshiki --cli name` prints the same name, and `--json` adds the status; see
the [command-line reference](agent-cli.md#name).

### Captions

The caption is ordinary editable text with an optional association to its
complete molecule. It follows layout changes. A chemical identity change
removes an obsolete automatic caption in the same Undo step; Undo restores it.
Editing the caption text or dragging it independently detaches it, preserving
that custom text through later chemical edits. Atom maps and visual atom
numbers do not change name identity.

`.rsk` save, reopen and whole-molecule copy keep the association. Copying a
caption alone keeps plain text. Figure export and CDXML/CDX keep visible
caption text; external formats and older ReShiki readers cannot keep the
association. The optional metadata uses the existing document-version
convention, so an older reader may save the caption as ordinary text.

### Alias catalog

The bundled catalog contains 40 retained or functional-class names for 30
identities, including `ethyl alcohol` for ethanol. Each alias displays its
source, version, record and license. The
[complete catalog and provenance](../src/naming/catalog/README.md) record the
selected OPSIN data (MIT license) and one ChEBI equivalence (CC BY 4.0). The S,
R and unspecified lactic acid graphs have separate entries. Neither charge nor
isotope variants inherit the neutral, unlabeled alias. Reaction map labels do
not change chemical identity. Looking up aliases needs no network connection,
Java, Python or other program. **Resolve name** is disabled while parsing and
for empty input. Changing preview chemistry prevents using aliases from the
original graph. After chemistry changes, generate a new name before using it.

### Records

The [desktop review and fixtures](changes/rust-chemical-naming.md) cover an
earlier build and an older design of the **Chemical names** inspector, with
examples, provenance and reproducible checks. The
[dock and molecule-caption record](changes/molecule-name-label.md) covers the
Import dock and captions, and the [alias review](changes/offline-name-synonyms.md)
covers the catalog.
