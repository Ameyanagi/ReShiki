# Predicted NMR shifts

Select an atom in one molecule, open **Properties → Predict NMR…**, and choose
**¹H** or **¹³C**. With no selection, the drawing must contain exactly one
molecular component. The complete connected graph is used, including atoms
behind collapsed abbreviations. Click a result row to select its linked drawing
atom and any explicitly drawn hydrogens. Expand an abbreviation to inspect an
internal site directly; its original atom ID is retained in the table.

Results first open in a compact floating palette at the full window’s right
edge, including when the inspector is shown. **Reset position** returns it there
without changing the camera. Drag its title to move it and
drag the visible bottom-right corner glyph to resize it. Drawing outside the
palette remains available, and opening or moving it leaves the drawing viewport
and zoom unchanged. **Dock** moves the same report into the inspector’s **NMR**
tab; **Undock** returns it to the floating palette. Docking preserves the camera
and restores the previous inspector state when undocked. Display mode, palette
position and label format are kept for the document’s current session.

The **Predicted shifts** plot places equal-height sticks at the supported shifts
on a descending ppm axis. Click a stick label or result row to select its linked
drawing atom. Nearby stick labels can be grouped; repeated clicks cycle their
atoms while distinct numerical positions remain visible. In **Details…**,
**Hide plot** makes the palette smaller; **Show plot** restores it. The **− / +**
size buttons and **Reset position** are also inside Details.

Use the **Show labels** checkbox to add temporary atom numbers to the drawing.
Its dropdown offers **Atom numbers**, **ppm** or **Both**: labels use bare
`#ID`, `shift ppm` or `#ID · shift ppm` text. The table and footer identify the
chosen nucleus and unresolved attached-H groups; labels do not individually
assign those protons. Labels follow geometry moves, appear only for visible
owners, and do not change the drawing, Undo history or exported figures. A narrow
soft-teal halo marks a selected supported NMR owner; its radius is unchanged.

Choose **Details…** to show reference counts, sphere radius, observed dispersion,
method and conditions. Longer results and details scroll inside the panel. The
header **−** control collapses the results; **+** expands them again. **×** closes
the panel and clears drawing labels. Chemical changes invalidate results and
labels; position/depth moves, drawing styles and abbreviation visibility preserve
the prediction. Running prediction again clears the previous labels. Changing
the selected molecule requires running prediction again. Results are per document
and background results cannot land in another document. See the
[floating-panel design and native review](changes/nmr-floating-mock-20261010.md).

**Copy** and **Export…** include a prediction label, atom IDs, method,
data version, conditions, limitations and data attribution. Predictions are not
stored as experimental assignments or full simulated spectra. Plot heights are
not intensity or integration. No multiplets, integrals, coupling constants,
second-order simulation or experimental peak assignment are claimed.

The offline method follows the nuclear-rooted spherical matching and
longest-sphere median fallback described in
[Ask Ernö (2016)](https://link.springer.com/article/10.1186/s13321-016-0134-6).
It uses an original, versioned ReShiki encoder, rather than the original
self-learning assignment loop. No Java, MySQL, network request, API key or
runtime training is required. Its numbers come from the separately licensed
[measured nmrshiftdb2 subset](../data/nmr/README.md); the raw unassigned spectra
from the Ask Ernö paper are not used as assigned reference data.

The first version supports neutral closed-shell ordinary-bond organic
molecules up to 128 atoms containing H, C, N, O, F, P, S, Cl, Br and I. It uses
recorded CDCl3 measurements at 273–323 K, pooled without temperature or
concentration correction. Charged structures, radicals, metals, multi-centre
attachments and non-default isotope labels are unsupported. A row without sufficient
matching data remains empty and explains why.

For **¹H**, one row represents the attached proton group of a parent drawing
atom. Implicit, explicit attached and explicitly bonded hydrogens are counted
consistently. Each group is linked to its parent atom and any explicit H atom
IDs. Carbon-bound proton groups receive a reference-group median when
supported. Exchangeable O–H/N–H/etc. rows remain without a numerical shift.
Multiple attached H are an **unresolved group**, not a claim that they form one
peak. The connectivity-only 2D encoder ignores E/Z and relative configurations; it
does not distinguish stereoisomers or assign individual
diastereotopic hydrogen shifts. Equivalent sites have the same environment
and prediction, but remain separate linked drawing rows.

For **¹³C**, each carbon receives one atom-linked row. The same data, matching
and limitations workflow applies.

Matching starts at sphere radius 4 around the nucleus, falling back to radius 3
then 2. At least two independent molecule/connectivity groups must support an
entry. **Sphere** reports the radius actually used; **Refs** reports independent
reference molecule groups. **Obs. SD** is the sample standard deviation of their
observed environment-group medians. It describes reference dispersion and is
**not a calibrated error bar or confidence interval**. The export also includes
the observed minimum/maximum.

Repeated spectra, symmetrical sites and stereoisomer records do not inflate
support: they contribute one median per molecular connectivity/environment.
Held-out experimental validation keeps duplicate molecules and stereoisomers
on the same side of the split. The measured coverage/error and preprocessing
counts are recorded in [validation.json](../data/nmr/validation.json).
Coverage is measured among accepted, assigned carbon/H-group targets with
recorded supported conditions; excluded chemistry and unknown-condition spectra
are counted separately in the audit. The held-out ¹H group-median MAE is 0.307 ppm at 92.5% coverage; ¹³C MAE is
2.05 ppm at 60.2% coverage. These are errors among supported accepted targets.
Those measurements come from the same
source release, not an independent
acquisition cohort; the metrics evaluate parent-site group medians. The
paper's published error is not claimed as ReShiki's accuracy.

The conditions filter can leave even familiar compounds partly unsupported.
Ethanol’s bundled result supports its methyl group, while its CH2 group lacks
sufficient qualified references and its exchangeable OH is unsupported.
The source ethanol CDCl3 proton spectrum has unreported temperature and was
excluded. Ethyl acetate has three supported non-exchangeable H groups and four
supported carbon sites in this release.
