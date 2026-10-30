# Predicted NMR shifts

NMR prediction estimates ¹H and ¹³C chemical shifts for one molecule from
measured reference data. It runs on your computer, and each value links to its
atom in the drawing.

## Use it

1. Select an atom of the molecule. If the drawing holds only one molecule, you
   can leave the selection empty.
2. Open **Properties → Predict NMR…**. The results open in a floating palette at
   the right edge of the window.
3. Choose **¹H** or **¹³C**.
4. Click a row or a stick label to select its atom in the drawing, together
   with any hydrogens drawn on it.
5. Turn on **Show labels** to write the results on the drawing. The menu next
   to it offers **Atom numbers** (`#3`), **ppm** (`1.280 ppm`) or **Both**
   (`#3 · 1.280 ppm`).
6. Choose **Copy** or **Export…** to keep the shifts with the method, data
   version, conditions, limitations and data credit.

### Move or dock the palette

- Drag the title to move the palette. Drag the corner glyph (◢) to resize it.
- **Dock** moves the report into the inspector, under **Predict → NMR**.
  **Undock** returns it to the palette. Clicking the **Predict** tab also docks
  the report.
- In the floating palette, **−** collapses the results and **+** expands them.
- **×** closes the report and removes its labels from the drawing.
- Opening, moving or docking the report never changes the zoom or the view.
  You can keep drawing while it is open.

### Read the results

- A ¹H row such as `C #3 · 2H` is the group of hydrogens on atom 3. A ¹³C row
  such as `C #3` is one carbon.
- The **Predicted shifts** plot marks each predicted shift with a stick on a
  ppm axis that runs from high to low.
- **▸ Details** adds a line to each row, for example
  `Shell 4 · 17 references · SD 0.064 ppm`. **Shell** is how far around the
  atom the match reaches, **references** is the number of independent
  reference molecules, and **SD** is the spread of their values. SD is not an
  error bar.
- A **Broad match** note appears directly under a value when the shortest
  supported shell pools a wide spread of reference shifts. The value stays in
  the table, plot and labels; the note also appears in copied and exported data.
- Details also shows the method, data version, conditions, limitations and
  data credit, and offers **Hide plot** / **Show plot**. In the floating
  palette it adds **Reset position** and the **−** / **+** size buttons.
- The footer names the nucleus and the solvent, for example
  `¹H · grouped per atom · CDCl₃`.

### What the plot and labels show

- The plot marks chemical shifts only. Stick heights do not show intensity or
  integration.
- J couplings, multiplets and second-order effects are not predicted.
- Nearby sticks share one label. Clicking the label again selects its next
  atom.
- Labels move with their atoms. An atom inside a collapsed abbreviation is
  labeled at the abbreviation. Labels never appear in exports or the Undo
  history.
- A soft teal halo marks a selected atom that has a prediction.

### When results update

- A chemical change clears the results and labels. Moving atoms, changing
  depth or drawing styles, and collapsing or expanding abbreviations keep them.
- To predict another molecule, select it and choose **Predict NMR…** again.
- Each document keeps its own report, palette position, docking and label
  format while it is open.

## Limits

- Molecules up to 128 atoms of H, C, N, O, F, P, S, Cl, Br and I. Charged
  molecules, radicals, metals, multi-center attachments and labels for less
  common isotopes, such as ²H or ¹³C, are not supported.
- References are CDCl₃ measurements recorded at 273–323 K. They are pooled
  without temperature or concentration correction.
- Only hydrogens on carbon get a value. O–H, N–H and other exchangeable
  hydrogens do not.
- Several hydrogens on one atom are one group value, not separate peaks.
- E/Z, relative configuration and stereoisomers are not distinguished.
  Diastereotopic hydrogens get the same value.
- A row with too little reference data stays empty and says why. Even simple
  molecules can be partly covered: in the ¹H result for ethanol only the CH₃
  group gets a value.
- A broad short-shell match can pool different chemical environments. For
  example, the bundled data gives paracetamol and acetanilide's acetyl CH₃
  group a median of 1.501 ppm with a **Broad match** note (Shell 2, SD
  0.507 ppm). This is a reference-group estimate, not an acetyl-specific value.
- Results are estimates, not assignments or simulated spectra.

## Implementation notes

### Method

The offline method follows the nuclear-rooted spherical matching and
longest-sphere median fallback described in
[Ask Ernö (2016)](https://link.springer.com/article/10.1186/s13321-016-0134-6).
It uses an original, versioned ReShiki encoder (`reshiki-hose-v1`), rather than
the original self-learning assignment loop. No Java, MySQL, network request, API
key or runtime training is required. Its numbers come from the separately
licensed [measured nmrshiftdb2 subset](../data/nmr/README.md); the raw
unassigned spectra from the Ask Ernö paper are not used as assigned reference
data.

The request uses the complete connected graph of the selected molecule,
including atoms behind collapsed abbreviations. Expand an abbreviation to see an
internal site directly; the table keeps its original atom ID. With no
selection, the drawing must contain exactly one molecular component. The first
version supports neutral closed-shell ordinary-bond organic molecules.

For **¹H**, one row represents the attached proton group of a parent drawing
atom. Implicit, explicit attached and explicitly bonded hydrogens are counted
consistently. Each group is linked to its parent atom and any explicit H atom
IDs. Carbon-bound proton groups receive a reference-group median when
supported. Exchangeable O–H/N–H rows remain without a numerical shift. Multiple
attached H are an unresolved group median, not a claim that they form one peak.
The connectivity-only 2D encoder ignores E/Z and relative configurations; it
does not distinguish stereoisomers or assign individual diastereotopic hydrogen
shifts. Equivalent sites have the same environment and prediction, but remain
separate linked rows. For **¹³C**, each carbon receives one atom-linked row with
the same matching and limitations.

Matching starts at sphere radius 4 around the nucleus, falling back to radius 3
then 2. At least two independent molecule/connectivity groups must support an
entry. **Shell** reports the radius actually used; **references** counts
independent reference molecule groups. **SD** is the sample standard deviation
of their observed environment-group medians. It describes reference dispersion
and is not a calibrated error bar or confidence interval. The export also
includes the observed minimum and maximum.

A supported radius-2 match is marked **Broad match** when its SD is at least
0.5 ppm for ¹H or 5 ppm for ¹³C. These descriptive cutoffs flag 49 of 151 ¹H
radius-2 entries (32.5%) and 224 of 2,176 ¹³C entries (10.3%) in the bundled
index; all of those entries meet the two-molecule support requirement. The
radius-2 SD medians are 0.362 and 1.577 ppm, respectively. The cutoffs expose
broad fallback groups while leaving narrower radius-2 matches unmarked. They
do not change the chosen reference median or coverage, and an unmarked value
is not a claim of calibrated accuracy.

### Coverage and accuracy

Repeated spectra, symmetrical sites and stereoisomer records do not inflate
support: they contribute one median per molecular connectivity/environment.
Held-out experimental validation keeps duplicate molecules and stereoisomers on
the same side of the split. The measured coverage/error and preprocessing
counts are recorded in [validation.json](../data/nmr/validation.json). Coverage
is measured among accepted, assigned carbon/H-group targets with recorded
supported conditions; excluded chemistry and unknown-condition spectra are
counted separately in the audit. The held-out ¹H group-median MAE is 0.307 ppm
at 92.5% coverage; ¹³C MAE is 2.05 ppm at 60.2% coverage. These are errors among
supported accepted targets. Those measurements come from the same source
release, not an independent acquisition cohort; the metrics evaluate
parent-site group medians. The paper's published error is not claimed as
ReShiki's accuracy.

The conditions filter can leave familiar compounds partly unsupported.
Ethanol's bundled result supports its methyl group, while its CH₂ group lacks
sufficient qualified references and its exchangeable OH is unsupported. The
source ethanol CDCl₃ proton spectrum has unreported temperature and was
excluded. Ethyl acetate has three supported non-exchangeable H groups and four
supported carbon sites in this release.

### Panel and labels

The palette opens at the full window's right edge, also when the inspector is
shown. **Reset position** returns it there without changing the camera. Docking
restores the previous inspector tab when undocked. Plot sticks have equal
height. Labels do not assign individual protons of an unresolved group; the
table and footer name the nucleus and the group. Running prediction again clears
the previous labels. Results are per document, and a background result cannot
land in another document. Copied and exported text includes a prediction label,
atom IDs, the method, data version, conditions, limitations and attribution.
Predictions are not stored as experimental assignments or full simulated
spectra. See the
[floating-panel design and desktop review](changes/nmr-floating-mock-20261010.md).
