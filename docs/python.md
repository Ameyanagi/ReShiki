# Python package

ReShiki's experimental Python API draws, converts and analyzes structures with
the editor's Rust engine. It requires Python 3.10 or later and runs without the
desktop application. Wheel publishing is a follow-up to
[issue #608](https://github.com/Ameyanagi/ReShiki/issues/608); this change provides
the package, local builds and Linux CI checks.

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
[`reshiki --cli analyze`](agent-cli.md#analyze): `formula`, `mass`, `exact_mass`,
`logp`, `tpsa`, `donors`, `acceptors`, `rings`, `inchi`, `inchikey`, `smiles` and
`unpaired_electrons`.

## Inputs and outputs

Use explicit constructors `from_smiles(text)`, `from_molfile(text)`,
`from_inchi(text)`, `from_cdxml(text)` and `from_name(name, online=False)`.
Names resolve offline. `online=True` raises `NotImplementedError` because the
existing PubChem client is part of the desktop app.

`reshiki.parse(text, format=None)` detects the input format. `Drawing.open(path)`
uses a file's extension, including extensionless POSCAR and CONTCAR names. It
accepts native `.rsk`, MOL/SDF, RXN, SMILES/reaction SMILES, InChI, CDXML, binary CDX,
CIF, PDB/ENT, POSCAR/CONTCAR/`.vasp` and XYZ/extXYZ. Crystal imports use the
[existing crystal readers](crystal-structures.md).

Render with `png()`, `svg()` or `pdf()`, and export with `cdxml()`, `mol()`,
`sdf()`, `smiles()`, `inchi()` or `rsk()`. PNG, PDF and native drawings return
bytes; chemical text and SVG return strings. `save(path, **options)` chooses the
output format from the extension. EMF raises `NotImplementedError` pending a
headless exporter.

```python
drawing = reshiki.Drawing.open("rutile.vasp")
drawing.save("rutile.svg")
png = reshiki.render("CCO", format="png", width=300)
mol = reshiki.convert("CCO", to="mol")
analysis = reshiki.properties("CCO")
```

## Styles and image dimensions

`reshiki.styles()` lists the editor's [document styles](drawing-styles.md).
Create a reusable `reshiki.Style("JACS / ACS", font_size=12)`, pass it as
`style=` to a render method, or supply overrides directly, such as
`molecule.svg(font_size=12)`. Overrides use points for physical dimensions.

PNG size follows the style's physical bond length and its default resolution.
Use `dpi=` to set resolution, or `width=` and/or `height=` to fit a pixel box
while preserving the aspect ratio.

## Batches and integrations

`render_many(items, format="png", errors="none", **options)` renders in parallel
while releasing Python's GIL. It returns a list with `None` for failed rows and
an `.errors` list of `(index, message)` pairs. Use `errors="raise"` to raise on
a failed row.

```python
pngs = reshiki.render_many(["CCO", "C1CC"], width=300)
print(pngs.errors)
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

Install `reshiki[pandas]` and explicitly import `reshiki.pandas` to register
the pandas Series accessor:

```python
import pandas as pd
import reshiki.pandas

series = pd.Series(["CCO", "O"])
images = series.reshiki.png(width=300)
descriptors = series.reshiki.properties()
```

The accessor provides `.png()`, `.svg()` and `.properties()`. Optional
dependencies load when their integration is used. RDKit interoperability uses
`Molecule.from_rdkit(mol)` and `molecule.to_rdkit()` through MOL blocks when
RDKit is installed.

Editable Office pictures depend on
[issue #462](https://github.com/Ameyanagi/ReShiki/issues/462). EMF export and
PyPI publishing with wheels for macOS, Windows and Linux on x64 and ARM64 remain
follow-ups.

## Build and check locally

From the repository root, with Rust and uv installed:

```sh
uvx maturin build --manifest-path crates/python/Cargo.toml --locked --out artifacts/python
uv sync --project crates/python --all-extras --no-install-project
uv run --project crates/python --no-sync --with ./artifacts/python/*.whl --with rdkit pytest crates/python/tests
uv run --project crates/python --no-sync ruff check crates/python/python crates/python/tests
uv run --project crates/python --no-sync ruff format --check crates/python/python crates/python/tests
uv run --project crates/python --no-sync ty check --project crates/python
```

The package version comes from the Rust workspace version. No desktop executable
or external InChI helper is required by the wheel.
