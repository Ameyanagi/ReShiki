# ReShiki for Python

Draw, convert and analyze chemical structures with the same Rust engine as the
ReShiki editor. Python 3.10 or later is required. The API is experimental until 1.0.

```sh
uv add reshiki  # or: pip install reshiki
```

```python
import reshiki

aspirin = reshiki.Molecule.from_smiles("CC(=O)Oc1ccccc1C(=O)O")
figure = aspirin.clean().with_style("JACS / ACS", font_size=12)
figure.save("aspirin.png", width=300)
print(aspirin.properties().formula)
```

Transforms return new objects. The original molecule keeps its geometry and
style. In Jupyter, displaying an object shows its SVG drawing.

Labels use the requested font when available, then the system's default sans-serif
font. An included sans-serif font keeps PNG and PDF labels visible even without
installed fonts; installing Liberation Sans is not required. To choose a specific
installed family, use, for example, `aspirin.with_style(font_family="DejaVu Sans")`.
After installing fonts, restart Python or the Jupyter kernel before rendering again.

## Reuse a style

Create a style context once and reuse its constructors and export functions:

```python
custom_style = reshiki.with_style("JACS / ACS", font_family="sans-serif", font_size=12)
aspirin = custom_style.Molecule.from_smiles("CC(=O)Oc1ccccc1C(=O)O")
aspirin.clean().save("aspirin.png", width=300)
images = custom_style.render_many(["CCO", "c1ccccc1O"], width=300)
```

The context also provides `Drawing`, `Reaction`, `parse`, `render` and `convert`.
It returns the usual immutable drawing objects. Configure a drawing before or
after construction: `custom_style.Molecule.from_smiles(text)` and
`reshiki.Molecule.from_smiles(text).with_style(custom_style.style)` apply the same
settings. `custom_style.with_style(font_size=14)` derives another context;
keyword overrides merge with its saved settings, and the latest value wins.
An explicit style name or `Style` selects a new base. Per-call export overrides
take precedence over context settings.

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
`from_inchi`, `from_cdxml` and `from_name`. Names resolve offline by default.
Use `reshiki.Molecule.from_name("aspirin", online=True)` or
`custom_style.Molecule.from_name("aspirin", online=True)` to query PubChem.
Online lookup requires one complete-name match and checks that importing its
stereospecific structure preserves molecular identity. Ambiguous names and
unsupported structures raise `ValueError`; network failures raise a connection
or timeout error. Each network request has a 15-second timeout.

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
EMF raises `NotImplementedError` pending a headless exporter. Wheels for macOS,
Windows and Linux on x64 and ARM64 follow each ReShiki release.

See the [Python guide](../../docs/python.md) for the API and local build commands.
