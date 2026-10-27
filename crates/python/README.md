# ReShiki for Python

Draw, convert and analyze chemical structures with the same Rust engine as the
ReShiki editor. Python 3.10 or later is required. The API is experimental until 1.0.

```python
import reshiki

aspirin = reshiki.Molecule.from_smiles("CC(=O)Oc1ccccc1C(=O)O")
figure = aspirin.clean().with_style("JACS / ACS", font_size=12)
figure.save("aspirin.png", width=300)
print(aspirin.properties().formula)
```

Transforms return new objects. The original molecule keeps its geometry and
style. In Jupyter, displaying an object shows its SVG drawing.

## Spreadsheets

Install the Excel integration with `uv add 'reshiki[excel]'`, then add drawings
beside a column of SMILES:

```python
import reshiki

reshiki.excel.add_structures(
    "compounds.xlsx",
    smiles="A",
    into="B",
    out="compounds-drawn.xlsx",
    width=300,
    header=True,
)
```

The helper uses openpyxl and embeds PNG images. Editable Office pictures are a
follow-up to [issue #462](https://github.com/Ameyanagi/ReShiki/issues/462).
Use `header=False` for a sheet whose first row contains a structure, or
`start_row=` to choose an explicit first row.

## Other entry points

```python
crystal = reshiki.Drawing.open("rutile.vasp")
svg = crystal.svg()
png = reshiki.render("c1ccccc1O", format="png", width=300)
mol = reshiki.convert("CCO", to="mol")
properties = reshiki.properties("CCO")
pngs = reshiki.render_many(["CCO", "C1CC"], format="png", width=300)
print(pngs.errors)
```

`reshiki.parse(text, format=None)` detects chemical text, native drawings and
crystal structures. Explicit constructors include `from_smiles`, `from_molfile`,
`from_inchi`, `from_cdxml` and offline `from_name`. An online name lookup raises
`NotImplementedError`: the current PubChem client belongs to the desktop app.

PNG dimensions follow the drawing style's physical bond length and `dpi` by
default. `width` and `height` optionally fit the image into a pixel box without
changing its aspect ratio. `reshiki.styles()` lists the app's drawing presets;
`reshiki.Style(name, **overrides)` makes a reusable style.

Install `reshiki[pandas]` and explicitly `import reshiki.pandas` to register
the pandas Series `.reshiki` accessor with `png()`, `svg()` and `properties()`.
Optional dependencies load when their integration is used.
RDKit conversion uses `Molecule.from_rdkit(mol)` and
`molecule.to_rdkit()` when RDKit is already installed.

Outputs include PNG, SVG, PDF, CDXML, MOL/SDF, SMILES, InChI and native `.rsk`.
EMF raises `NotImplementedError` pending a headless exporter. This PR does not
publish to PyPI; six-platform release wheels and publishing are follow-ups.

See the [Python guide](../../docs/python.md) for the API and local build commands.
