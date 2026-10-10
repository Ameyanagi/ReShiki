# Offline NMR reference data

`index.tsv` is a derivative database made from the **2026-03-15**
`nmrshiftdb2withsignals.sd` export of nmrshiftdb2. It is licensed under the
**nmrshiftdb2 Database License**, separately from ReShiki's original code.
The full license is [DATABASE-LICENSE.txt](../../licenses/nmrshiftdb2/DATABASE-LICENSE.txt).
Copyright and database rights remain with the nmrshiftdb2 contributors.

Contains information from [nmrshiftdb2](https://nmrshiftdb.nmr.uni-koeln.de),
which is made available here under the nmrshiftdb2 Database License.

Source: <https://downloads.sourceforge.net/project/nmrshiftdb2/data/nmrshiftdb2withsignals.sd>
(158,569,118 bytes), SHA-256:
`0e86688360e23c88ccf0eb82a1251315fa57ec6f5b376dc8886f3311793a0afe`.
The data license is distributed alongside the upstream snapshots at
<https://svn.code.sf.net/p/nmrshiftdb2/code/trunk/snapshots/nmrshiftdb2datalicense.txt>
(SVN revision 2665 at retrieval, 2026-10-09; license SHA-256
`00322cc450c2e53e305c082e1ab7454f108b7df51c0dbee0a07932666b245516`).
It defines the licensed database to include its SDF/CML/SQL forms. The upstream
software's AGPL code and Java predictor are not included or adapted.

The machine-readable derivative index, exact transformation implementation
[`dataset.rs`](../../crates/io/src/nmr/dataset.rs), production encoder
[`nmr.rs`](../../crates/chemistry/src/nmr.rs), and regeneration script are
provided in this repository. This makes the derivative database and the method
of alteration available under the data license; the original implementation
code retains ReShiki's MIT OR Apache-2.0 license, as permitted by section 4.5.

Regenerate with `python3 scripts/regenerate_nmr.py --source /path/to/source.sd`,
or omit `--source` to download the pinned export. A changed source digest is
rejected. No download or training occurs in the application.

The transformation requires exact recorded CDCl3 solvent and numeric
273–323 K temperature, excludes indexed calculation metadata, and accepts
neutral closed-shell ordinary-bond organic graphs of at most 128 atoms.
Only assigned carbon-bound proton groups and carbon shifts enter the index.
Exchangeable protons are excluded. Unsupported graphs or assignments are
excluded instead of repaired heuristically. Duplicate spectra, stereoisomer
records, and symmetric sites contribute one connectivity/environment median
per independent molecule group. Index entries with fewer than two independent
molecule groups are omitted.

The encoder is **reshiki-hose-v1**, an original nuclear-rooted spherical graph
encoding with explicit ring connectivity and full attached-H counts. Each
sphere uses native canonical graph ranking and is hashed with SHA-256.
It is not interchangeable with CDK, OpenChemLib, or the legacy Ask Ernö
HOSE strings. Neither externally supplied HOSE strings nor an unlicensed corpus
is imported by the application.

`validation.json` reports held-out experimental coverage/error, with all
molecules sharing an InChI connectivity block in the same deterministic split.
The released index then uses the full accepted source subset. Validation is
against held-out measurements from the same export, not an independently
acquired experimental cohort. Multiple attached protons are evaluated as group
medians; accuracy metrics must not be interpreted as stereospecific H accuracy.

## Held-out validation

| Nucleus / target                  | Held-out molecules | Predicted groups / assigned targets | Coverage | MAE / ppm | Median AE / ppm | RMSE / ppm |
| --------------------------------- | -----------------: | ----------------------------------: | -------: | --------: | --------------: | ---------: |
| ¹H attached-C proton-group median |                126 |                           545 / 589 |   92.53% |    0.3065 |          0.1625 |     0.5156 |
| ¹³C atom shift                    |                309 |                         2207 / 3665 |   60.22% |    2.0466 |          1.0000 |     3.5214 |

These errors are conditional on supported predictions, and coverage is among
accepted assigned targets at the supported conditions. The audit counts 58,187
source records, 8,417 computed spectra excluded, 50,403 spectra with unsupported
or unknown conditions excluded, 171 chemistry records rejected and two spectra
with unsupported assignments rejected. The index uses 2,444 accepted spectra
from 1,943 independent connectivity groups, containing 28,754 assigned site
observations before connectivity/environment aggregation. Its 6,478 entries
retain at least two independent reference groups each.

Do not substitute these results for the original paper's accuracy or for
stereospecific individual-hydrogen accuracy. Observed SD is not a calibrated
uncertainty measure. Source errors, sparse environments, experimental conditions
and 2D stereochemical ambiguity remain limitations.

The strict conditions can leave familiar molecules partly unsupported. For
example, the source ethanol record (molecule 10009222) has an assigned CDCl3
¹H spectrum with unreported temperature; its known-temperature ¹H spectrum
uses D2O. These observations are excluded. In the bundled index ethanol has a
supported methyl group but its CH2 group lacks the required two qualified
independent connectivity references; its exchangeable OH group is explicitly
unsupported. Ethyl acetate is a small-molecule acceptance example with all three
non-exchangeable H groups and four carbon sites supported.
