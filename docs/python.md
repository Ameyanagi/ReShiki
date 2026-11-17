# Python package

ReShiki's experimental Python API draws, converts and analyzes structures with
the editor's Rust engine. It requires Python 3.10 or later and runs without the
desktop application. Install it with `uv add reshiki` or `pip install reshiki`.
From 0.14.0, each ReShiki release publishes abi3 wheels
with the app's version for macOS, Windows and Linux on x64 and ARM64; see
[Releasing](releasing.md#configure-pypi-publishing).

## Draw and analyze

```python
import reshiki

aspirin = reshiki.Molecule.from_smiles("CC(=O)Oc1ccccc1C(=O)O")
styled = aspirin.clean().with_style("JACS / ACS", font_size=12)
styled.save("aspirin.png", width=300)
svg = styled.svg()
pdf = styled.pdf()
print(aspirin.properties().formula)
```

`Molecule`, `Reaction` and `Drawing` are immutable. `clean()` and
`with_style(style, **overrides)` return new objects, so they can be chained.
Jupyter displays each object as SVG.

`properties()` returns a frozen dataclass with the same fields as
[`reshiki --cli analyze`](agent-cli.md#analyze): `formula`, `display_formula`,
`mass`, `exact_mass`, `logp`, `tpsa`, `donors`, `acceptors`, `rings`, `inchi`,
`inchikey`, `smiles` and `unpaired_electrons`. `mass`, `exact_mass`, `logp`
and `tpsa` are rounded to 9 decimal places, as the command line and the MCP
tools report them.

## Predict NMR

`predict_nmr("1H")` or `predict_nmr("13C")` predicts chemical shifts offline,
as **Properties → Predict NMR…** does in the app. The drawing must be exactly
one molecule. The nucleus is required.

```python
ethyl_acetate = reshiki.Molecule.from_smiles("CCOC(C)=O")
carbons = ethyl_acetate.predict_nmr("13C")
for row in carbons:
    print(row.atom, row.ppm)
print(carbons.credit)
```

It returns a frozen `NmrPrediction` with the same fields as
[`reshiki --cli predict-nmr`](agent-cli.md#predict-nmr) and the MCP
`predict_nmr` tool, except the document revision. Iterating over it yields
its `rows`, one `NmrShift` per site in drawing order:

| Field                 | Meaning                                                              |
| --------------------- | -------------------------------------------------------------------- |
| `atom`                | The drawing's atom ID; for ¹H, the heavy atom that carries the H     |
| `nucleus`             | `1H` or `13C`                                                        |
| `ppm`                 | The predicted shift: the median of the matched reference group       |
| `shell`, `references` | The sphere radius of the match and the number of reference molecules |
| `sd`, `min`, `max`    | The spread of the reference shifts, in ppm                           |
| `broad`               | The shortest sphere matched a group with a wide spread               |
| `hydrogen_count`      | The number of attached H, for ¹H; `None` for ¹³C                     |
| `hydrogens`           | The IDs of attached H drawn as atoms                                 |
| `note`                | What **Export…** writes in its Limitations column, or `None`         |

`ppm` through `max` are `None` for a site without reference data, and `note`
says why. The prediction also carries `nucleus`, `solvent` (`CDCl3`), `method`,
`data_version`, `conditions`, `limitations`, `credit` and `data_access`, as
**Export…** writes them.

Atom IDs follow the input. For SMILES, atom `N` is the `N`th atom after
explicit H are removed as RDKit does, which is index `N - 1` in RDKit's
`Chem.MolFromSmiles`. For a MOL
file, it is the atom's position in the atom block, counting from 1: the V2000
atom number (V3000 record numbers are not kept). A `.rsk` drawing keeps its stored
IDs, which can have gaps. `clean()` keeps IDs.

A drawing that is not one molecule, unsupported chemistry (such as metals,
charges or isotope labels) and a molecule without sites for the nucleus raise
`ReShikiError` with code `failed`; a nucleus other than `1H` or `13C` raises it
with code `invalid_arguments`.

The reference data comes from nmrshiftdb2 under the
[nmrshiftdb2 Database License](https://github.com/Ameyanagi/ReShiki/blob/main/licenses/nmrshiftdb2/DATABASE-LICENSE.txt).
When you publish predicted values, cite `credit` with them. See
[NMR prediction](nmr-prediction.md) for the method and its limits.

## Inputs and outputs

Use explicit constructors `from_smiles(text)`, `from_molfile(text)`,
`from_inchi(text)`, `from_cdxml(text)` and `from_name(name, online=False)`.
Names resolve offline by default. Pass `online=True` to look up a complete name
in PubChem instead:

```python
aspirin = reshiki.Molecule.from_name("aspirin", online=True)
```

Online lookup requires a unique complete-name match and imports stereospecific
SMILES without dropping components. ReShiki verifies that layout preserves its
molecular identity; ambiguous or unsupported results raise `ValueError`.
Connection failures and request timeouts raise their corresponding Python
exceptions. Each network request has a 15-second timeout. Offline lookup never
contacts PubChem.

`reshiki.parse(text, format=None)` detects the input format. `Drawing.open(path)`
uses a file's extension, including extensionless POSCAR and CONTCAR names. It
accepts native `.rsk`, MOL/SDF, RXN, SMILES/reaction SMILES, InChI, CDXML, binary CDX,
CIF, PDB/ENT, POSCAR/CONTCAR/`.vasp` and XYZ/extXYZ. Crystal imports use the
[existing crystal readers](crystal-structures.md).

Make figures with `png()`, `svg()` or `pdf()`, and chemistry files with
`cdxml()`, `cdx()`, `mol()`, `sdf()`, `smiles()`, `inchi()` or `rsk()`. A
reaction also has `rxn()` and `rsmi()` (reaction SMILES). PNG, PDF, CDX and
`.rsk` drawings return bytes; chemical text and SVG return strings.
`export(format, **options)` writes any of these by name; it is what the app's
**Export** command, the MCP `export` tool and `reshiki --cli convert` make.
`render()` is the older name of `export()` and stays. EMF raises
`NotImplementedError` pending a headless exporter.

`save(path, **options)` chooses the output format from the extension. It
replaces an existing file; pass `overwrite=False` to get `FileExistsError`
instead, as the MCP `file_save` tool refuses without `overwrite` and the
command line without `--force`.

SVG output draws text as outlines, so every viewer shows the glyphs, positions
and sizes that `png()` draws, whatever fonts it has. Pass `svg_text="live"` to
keep `<text>` elements that stay editable and searchable; a viewer without
Arial then substitutes its own font, and labels can look narrower or wider
than in the PNG. `svg()`, `export()`, `convert()`, `save()` and
`render_many()` accept `svg_text=`, the name the MCP tools and the command
line use; other formats ignore it. The older spelling `text=` still works.

## Errors

ReShiki raises `reshiki.ReShikiError`, a `ValueError`, so `except ValueError`
keeps working. Its `code` is the [error code](agent-api.md#error-codes) the MCP
tools and the command line report for the same failure:

| `code`              | Meaning                                                                                 |
| ------------------- | --------------------------------------------------------------------------------------- |
| `invalid_arguments` | A format, option or style is not accepted, or a constructor got another kind of content |
| `failed`            | The structure could not be read, drawn or written                                       |

```python
try:
    reshiki.Molecule.from_smiles("C(")
except reshiki.ReShikiError as error:
    print(error.code, error)  # failed Invalid SMILES at byte 2
```

A wrong argument type still raises `TypeError`, and a missing file `OSError`.

```python
drawing = reshiki.Drawing.open("rutile.vasp")
drawing.save("rutile.svg")
png = reshiki.export("CCO", format="png", width=300)
mol = reshiki.convert("CCO", to="mol")
analysis = reshiki.properties("CCO")
```

## Import warnings

Some inputs import with a change or an assumption: a SMILES bond of any order
(`~`) is drawn as a single bond, and stereo on an extended double bond
(C=C=C=C) is dropped. The app shows such a warning in its
status bar and `reshiki --cli` prints it as `reshiki: warning:`. The Python
package issues it as a `reshiki.StructureWarning`, a `UserWarning`, through
Python's `warnings` module, attributed to your line of code:

```python
molecule = reshiki.Molecule.from_smiles("C~C")
# demo.py:3: StructureWarning: Bond C1–C2 has any order (~); bond order assumed
# single
```

The import still returns the drawing. Python's default filter shows a warning
once per line of code. The standard `warnings` filters collect warnings, hide
them, or turn them into errors:

```python
import warnings

with warnings.catch_warnings(record=True) as caught:
    warnings.simplefilter("always", reshiki.StructureWarning)
    molecule = reshiki.Molecule.from_smiles(text)
messages = [str(warning.message) for warning in caught]

warnings.simplefilter("error", reshiki.StructureWarning)  # refuse such inputs
```

Batches report warnings with the results; see
[Batches and integrations](#batches-and-integrations).

## Styles and image dimensions

`reshiki.styles()` lists the editor's [document styles](drawing-styles.md).
Create a reusable `reshiki.Style("JACS / ACS", font_size=12)`, pass it as
`style=` to a render method, or supply overrides directly, such as
`molecule.svg(font_size=12)`. Overrides use points for physical dimensions.

Bind a style once to reuse it across constructors and functional exports:

```python
custom_style = reshiki.with_style("JACS / ACS", font_family="sans-serif", font_size=12)
aspirin = custom_style.Molecule.from_smiles("CC(=O)Oc1ccccc1C(=O)O")
named = custom_style.Molecule.from_name("aspirin", online=True)
svg = custom_style.render("CCO", "svg", width=300)
pngs = custom_style.render_many(["CCO", "c1ccccc1O"], width=300)
```

The context offers `Drawing`, `Molecule` and `Reaction` constructors and the same
`parse`, `render`, `convert` and `render_many` operations as the module. Returned
objects keep their normal types. The equivalent object-first form is
`reshiki.Molecule.from_smiles(text).with_style(custom_style.style)`.

Contexts are immutable. `custom_style.with_style(font_size=14)` combines new
overrides with the saved settings; the latest value for a setting wins. Passing
an explicit style name or `Style` selects a new base. Per-call export overrides
take precedence over the context's settings. Configuration can be applied before
or after construction; `clean()` remains an operation on the drawing.
Saved styles include explicit default values, so applying a reset context also
turns off settings already enabled on a drawing. Keyword-only updates to an
individual drawing preserve its other settings.

Labels use the requested font when available, then generic sans-serif. Where
Arial is not installed, as on most Linux systems and containers, ReShiki draws
Arial with its bundled Liberation Sans, which has Arial's character widths.
Labels are then laid out as on macOS and Windows, even with no fonts installed
at all. The same faces stand in for Helvetica when it is missing too. An
installed Arial always takes precedence. After installing system fonts, restart Python or the Jupyter
kernel before rendering again.

Aspirin rendered with `png(width=640)` in a `python:3.12-slim` container shows
the effect. The before images use PyPI 0.14.0. With the bundled faces, both
containers lay the labels out as macOS does with its real Arial (on the right);
only the letter shapes of Liberation Sans and Arial differ slightly.

| Font environment   | 0.14.0                                                                                            | Bundled Arial fallback                                                                             | macOS, real Arial                                                         |
| ------------------ | ------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| No fonts installed | ![Aspirin loses its atom labels without fonts](images/python-headless-fonts/no-fonts-before.png)  | ![Aspirin keeps Arial-width labels without fonts](images/python-headless-fonts/no-fonts-after.png) | ![Aspirin with macOS Arial](images/python-headless-fonts/macos-arial.png) |
| DejaVu Sans only   | ![Aspirin labels drawn wider in DejaVu Sans](images/python-headless-fonts/dejavu-only-before.png) | ![Aspirin labels at Arial widths](images/python-headless-fonts/dejavu-only-after.png)              | ![Aspirin with macOS Arial](images/python-headless-fonts/macos-arial.png) |

PNG size follows the style's physical bond length and its default resolution.
Use `dpi=` to set resolution, or `width=` and/or `height=` to fit a pixel box
while preserving the aspect ratio.

## Batches and integrations

`render_many(items, format="png", errors="none", **options)` renders in parallel
while releasing Python's GIL. It returns a list with `None` for failed rows and
an `.errors` list of `(index, message)` pairs. Use `errors="raise"` to raise on
a failed row. Import warnings are not issued as Python warnings here: the
result's `.warnings` lists them as `(index, message)` pairs, in row order.

```python
pngs = reshiki.render_many(["CCO", "C1CC", "C~C"], width=300)
print(pngs.errors)    # [(1, '...')]
print(pngs.warnings)  # [(2, 'Bond C1–C2 has any order (~); bond order assumed single')]
```

The Excel extra uses openpyxl and embeds PNG images:

```python
import reshiki

reshiki.excel.add_structures(
    "compounds.xlsx", smiles="A", into="B",
    out="compounds-drawn.xlsx", width=300, header=True,
)
```

Install `reshiki[excel]` for this helper. The header defaults to row 1; use
`header=False` to render from row 1, or `start_row=` to choose the first row.
Import warnings are issued as `StructureWarning`s that start with the source
cell, such as `A3: Bond C1–C2 has any order (~); bond order assumed single`.

Install `reshiki[pandas]` and explicitly import `reshiki.pandas` to register
the pandas Series accessor:

```python
import pandas as pd
import reshiki.pandas

series = pd.Series(["CCO", "O"])
images = series.reshiki.png(width=300)
descriptors = series.reshiki.properties()
```

The accessor provides `.png()`, `.svg()` and `.properties()`. `.png()` and
`.svg()` keep the batch's errors and import warnings in the result's
`attrs["reshiki_errors"]` and `attrs["reshiki_warnings"]`; `.properties()`
reads each row on its own and issues `StructureWarning`s. Optional
dependencies load when their integration is used. RDKit interoperability uses
`Molecule.from_rdkit(mol)` and `molecule.to_rdkit()` through MOL blocks when
RDKit is installed.

Editable Office pictures depend on
[issue #462](https://github.com/Ameyanagi/ReShiki/issues/462). EMF export remains
a follow-up.

## Build and check locally

From the repository root, with Rust and uv installed:

```sh
uv run --no-project --python 3.12.15 python scripts/license_notices.py
uvx maturin@1.15.0 build --manifest-path crates/python/Cargo.toml --locked --out artifacts/python
uv run --no-project --python 3.12.15 python scripts/check_python_wheel_notices.py 'artifacts/python/*.whl'
uv sync --project crates/python --all-extras --no-install-project
uv run --project crates/python --no-sync --with ./artifacts/python/*.whl --with rdkit pytest crates/python/tests
uv run --project crates/python --no-sync ruff check crates/python/python crates/python/tests
uv run --project crates/python --no-sync ruff format --check crates/python/python crates/python/tests
uv run --project crates/python --no-sync ty check --project crates/python
```

The package version comes from the Rust workspace version. No desktop executable
or external InChI helper is required by the wheel.
The ignored `crates/python/wheel-licenses/` staging directory contains the project
licenses and consolidated notices for the selected target's Rust dependencies.
Pass `--target` to the notice generator when cross-compiling. Maturin includes
these files under the wheel's `.dist-info/licenses/` and declares them with
`License-File` metadata.
