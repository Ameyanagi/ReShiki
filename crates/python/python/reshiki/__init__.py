"""Immutable chemical drawings backed by the same engine as the ReShiki app.

Text exports (SVG, CDXML, MOL, SDF, SMILES and InChI) return ``str``;
PNG, PDF and RSK exports return ``bytes``. Optional integrations are loaded
only when used; import ``reshiki.pandas`` to register the pandas accessor.
"""

from __future__ import annotations

import importlib
import json
import math
import sys
import warnings
from dataclasses import dataclass, field
from os import PathLike
from pathlib import Path
from types import MappingProxyType, ModuleType
from typing import TYPE_CHECKING, Any, Generic, Iterable, Literal, Mapping, TypeAlias, TypeVar, cast

from . import _native

if TYPE_CHECKING:
    from . import excel as excel

__version__ = _native.version()
__all__ = [
    "Drawing",
    "Molecule",
    "Reaction",
    "Style",
    "StyleContext",
    "Properties",
    "RenderResults",
    "StructureWarning",
    "parse",
    "render",
    "convert",
    "properties",
    "render_many",
    "styles",
    "with_style",
    "excel",
]

_TEXT_FORMATS = frozenset({"svg", "cdxml", "mol", "sdf", "smiles", "inchi"})
_OUTPUT_FORMATS = _TEXT_FORMATS | {"png", "pdf", "rsk", "emf"}
_FORMAT_ALIASES = {
    "smi": "smiles",
    "molfile": "mol",
    "reshiki": "rsk",
    "moruno": "rsk",
    "ent": "pdb",
    "vasp": "poscar",
    "contcar": "poscar",
    "extxyz": "xyz",
    "reaction_smiles": "rsmi",
}
_STYLE_ALIASES = {
    "font_size": "font_size_pt",
    "bond_length": "bond_length_pt",
    "line_width": "line_width_pt",
    "bold_width": "bold_width_pt",
    "margin_width": "margin_width_pt",
    "crossing_clearance": "crossing_clearance_pt",
    "hash_spacing": "hash_spacing_pt",
    "bond_spacing": "bond_spacing_ratio",
}
_D = TypeVar("_D", bound="Drawing")


class StructureWarning(UserWarning):
    """The structure was imported, but ReShiki changed or assumed part of it; the message says what."""


def _format(value: str) -> str:
    if not isinstance(value, str):
        raise TypeError("format must be a string")
    name = value.lower().lstrip(".")
    return _FORMAT_ALIASES.get(name, name)


def _output_format(value: str) -> str:
    name = _format(value)
    if name == "emf":
        raise NotImplementedError(
            "EMF output is planned in follow-up #462 (editable Office pictures)"
        )
    if name not in _OUTPUT_FORMATS:
        raise ValueError(f"Unsupported output format: {value!r}")
    return name


def _data(value: str | bytes) -> bytes:
    if isinstance(value, str):
        return value.encode("utf-8")
    if isinstance(value, bytes):
        return value
    raise TypeError("structure input must be text or bytes")


def _read_path(
    path: str | PathLike[str], format: str | None = None
) -> tuple[bytes, str, list[str]]:
    source = Path(path)
    inferred = source.suffix.lstrip(".")
    if not inferred and source.name.upper() in {"POSCAR", "CONTCAR"}:
        inferred = "poscar"
    name = _format(format or inferred or "auto")
    return _native.parse(source.read_bytes(), name)


def _from_native(result: tuple[bytes, str, list[str]]) -> Drawing:
    data, kind, messages = result
    cls = {"molecule": Molecule, "reaction": Reaction, "drawing": Drawing}.get(kind)
    if cls is None:
        raise RuntimeError(f"Native engine returned unknown drawing kind: {kind!r}")
    if messages:
        frame = sys._getframe()
        stacklevel = 1
        package = Path(__file__).resolve().parent
        while frame is not None and Path(frame.f_code.co_filename).resolve().is_relative_to(
            package
        ):
            stacklevel += 1
            frame = frame.f_back
        del frame
        for message in messages:
            warnings.warn(message, StructureWarning, stacklevel=stacklevel)
    return cls(data)


def _from_constructor(cls: type[_D], result: tuple[bytes, str, list[str]]) -> _D:
    drawing = _from_native(result)
    if cls is Drawing:
        return cast(_D, drawing)
    expected: type[Drawing]
    if issubclass(cls, Molecule):
        expected = Molecule
    elif issubclass(cls, Reaction):
        expected = Reaction
    else:
        expected = Drawing
    if not isinstance(drawing, expected):
        raise ValueError(
            f"{cls.__name__} requires {expected.__name__.lower()} content; "
            f"received {type(drawing).__name__.lower()}"
        )
    return cls(drawing._data)


@dataclass(frozen=True, init=False)
class Style:
    """A validated app document style, optionally with keyword overrides.

    Common overrides include ``font_size``, ``font_family``, ``bond_length``,
    ``line_width``, ``bold_width``, ``margin_width``, ``crossing_clearance`` and
    ``bond_spacing``.
    Sizes use points; ``bond_spacing`` is a ratio.
    """

    _json: str = field(repr=False)

    def __init__(self, name: str = "JACS / ACS", **overrides: Any) -> None:
        encoded = {_STYLE_ALIASES.get(key, key): value for key, value in overrides.items()}
        object.__setattr__(self, "_json", _native.style(name, json.dumps(encoded)))

    @property
    def name(self) -> str:
        return str(self.as_dict()["name"])

    @property
    def values(self) -> Mapping[str, Any]:
        """An immutable view of the complete style settings."""
        return MappingProxyType(self.as_dict())

    def as_dict(self) -> dict[str, Any]:
        """Return an independent dictionary of native style settings."""
        return json.loads(self._json)

    def __repr__(self) -> str:
        return f"Style({self.name!r})"


def styles() -> dict[str, Style]:
    """Return the app's built-in document styles, keyed by display name."""
    return {name: Style(name) for name in json.loads(_native.styles())}


def _style_json(style: str | Style | None, overrides: Mapping[str, Any]) -> str | None:
    overrides = {_STYLE_ALIASES.get(key, key): value for key, value in overrides.items()}
    if style is None:
        return json.dumps(dict(overrides)) if overrides else None
    if isinstance(style, str):
        return Style(style, **overrides)._json
    if isinstance(style, Style):
        if not overrides:
            return style._json
        values = style.as_dict()
        if "bond_length_pt" in overrides and "bond_length_world" not in overrides:
            # Let the native engine recalculate world units for the new length.
            values.pop("bond_length_world", None)
        return json.dumps({**values, **overrides})
    raise TypeError("style must be a style name or reshiki.Style")


def _size(width: int | None, height: int | None, dpi: float | None) -> None:
    for name, value in (("width", width), ("height", height)):
        if value is not None and (
            isinstance(value, bool) or not isinstance(value, int) or value <= 0
        ):
            raise ValueError(f"{name} must be a positive integer")
    if dpi is not None and (
        isinstance(dpi, bool)
        or not isinstance(dpi, (int, float))
        or not math.isfinite(dpi)
        or dpi <= 0
    ):
        raise ValueError("dpi must be a positive finite number")


@dataclass(frozen=True)
class Properties:
    """Molecular descriptors returned by the headless ReShiki engine."""

    formula: str
    display_formula: str
    mass: float
    exact_mass: float
    logp: float
    tpsa: float
    donors: int
    acceptors: int
    rings: int
    inchi: str
    inchikey: str
    smiles: str
    unpaired_electrons: int = 0
    smiles_notice: str | None = None


@dataclass(frozen=True)
class Drawing:
    """An immutable ReShiki document. Transforms return a new document."""

    _data: bytes = field(repr=False)

    @classmethod
    def open(cls: type[_D], path: str | PathLike[str], *, format: str | None = None) -> _D:
        """Read a structure file, including extensionless POSCAR and CONTCAR."""
        return _from_constructor(cls, _read_path(path, format))

    @classmethod
    def from_smiles(cls: type[_D], smiles: str) -> _D:
        """Parse molecular or reaction SMILES explicitly."""
        data = _data(smiles)
        format = "rsmi" if b">" in data else "smiles"
        return _from_constructor(cls, _native.parse(data, format))

    @classmethod
    def from_molfile(cls: type[_D], molfile: str | bytes) -> _D:
        return _from_constructor(cls, _native.parse(_data(molfile), "mol"))

    @classmethod
    def from_inchi(cls: type[_D], inchi: str) -> _D:
        return _from_constructor(cls, _native.parse(_data(inchi), "inchi"))

    @classmethod
    def from_cdxml(cls: type[_D], cdxml: str | bytes) -> _D:
        return _from_constructor(cls, _native.parse(_data(cdxml), "cdxml"))

    @classmethod
    def from_name(cls: type[_D], name: str, *, online: bool = False) -> _D:
        """Resolve a chemical name offline, or with PubChem when ``online=True``."""
        if not isinstance(online, bool):
            raise TypeError("online must be a boolean")
        if online:
            from ._online import from_name

            return _from_constructor(cls, from_name(name))
        return _from_constructor(cls, _native.from_name(name))

    def clean(self: _D) -> _D:
        """Return a drawing with freshly generated chemical coordinates."""
        return type(self)(_native.clean(self._data))

    def with_style(self: _D, style: str | Style | None = None, **overrides: Any) -> _D:
        """Return a drawing with a named style and/or style overrides."""
        encoded = _style_json(style, overrides)
        if encoded is None:
            return type(self)(self._data)
        return type(self)(_native.with_style(self._data, encoded))

    def render(
        self,
        format: str = "png",
        *,
        style: str | Style | None = None,
        width: int | None = None,
        height: int | None = None,
        dpi: float | None = None,
        text: Literal["outline", "live"] = "outline",
        **overrides: Any,
    ) -> bytes | str:
        """Export this document; optional dimensions fit within a pixel box.

        SVG text is drawn as outlines; text='live' keeps <text> elements.
        """
        name = _output_format(format)
        _size(width, height, dpi)
        result = _native.render(
            self._data, name, _style_json(style, overrides), width, height, dpi, svg_text=text
        )
        return result.decode("utf-8") if name in _TEXT_FORMATS else result

    def png(
        self,
        *,
        style: str | Style | None = None,
        width: int | None = None,
        height: int | None = None,
        dpi: float | None = None,
        **overrides: Any,
    ) -> bytes:
        return cast(
            bytes,
            self.render("png", style=style, width=width, height=height, dpi=dpi, **overrides),
        )

    def svg(
        self,
        *,
        style: str | Style | None = None,
        width: int | None = None,
        height: int | None = None,
        dpi: float | None = None,
        text: Literal["outline", "live"] = "outline",
        **overrides: Any,
    ) -> str:
        """SVG text is drawn as outlines; text='live' keeps <text> elements."""
        return cast(
            str,
            self.render(
                "svg", style=style, width=width, height=height, dpi=dpi, text=text, **overrides
            ),
        )

    def pdf(
        self,
        *,
        style: str | Style | None = None,
        width: int | None = None,
        height: int | None = None,
        dpi: float | None = None,
        **overrides: Any,
    ) -> bytes:
        return cast(
            bytes,
            self.render("pdf", style=style, width=width, height=height, dpi=dpi, **overrides),
        )

    def emf(self, **options: Any) -> bytes:
        return cast(bytes, self.render("emf", **options))

    def cdxml(self, **options: Any) -> str:
        return cast(str, self.render("cdxml", **options))

    def mol(self, **options: Any) -> str:
        return cast(str, self.render("mol", **options))

    def sdf(self, **options: Any) -> str:
        return cast(str, self.render("sdf", **options))

    def smiles(self, **options: Any) -> str:
        return cast(str, self.render("smiles", **options))

    def inchi(self, **options: Any) -> str:
        return cast(str, self.render("inchi", **options))

    def rsk(self, **options: Any) -> bytes:
        return cast(bytes, self.render("rsk", **options))

    def save(self, path: str | PathLike[str], *, format: str | None = None, **options: Any) -> Path:
        """Export to a file, choosing the format from its extension by default."""
        destination = Path(path)
        name = format or destination.suffix.lstrip(".")
        if not name:
            raise ValueError("A filename extension or explicit format is required")
        output = self.render(name, **options)
        destination.write_bytes(output.encode("utf-8") if isinstance(output, str) else output)
        return destination

    def properties(self) -> Properties:
        fields = json.loads(_native.properties(self._data))
        # Like the CLI analysis JSON, the public descriptors leave out the inspector's mass percentages.
        del fields["mass_percent"]
        return Properties(**fields)

    def _repr_svg_(self) -> str:
        return self.svg()


@dataclass(frozen=True)
class Molecule(Drawing):
    """A molecular drawing, with optional RDKit interoperability via MOL blocks."""

    @classmethod
    def from_rdkit(cls, molecule: Any) -> Molecule:
        chem = _rdkit()
        if molecule is None:
            raise ValueError("RDKit molecule must not be None")
        return cls.from_molfile(chem.MolToMolBlock(molecule))

    def to_rdkit(self) -> Any:
        chem = _rdkit()
        result = chem.MolFromMolBlock(self.mol(), removeHs=False)
        if result is None:
            raise ValueError("RDKit could not read the exported MOL block")
        return result


@dataclass(frozen=True)
class Reaction(Drawing):
    """A reaction drawing."""

    @classmethod
    def from_rxn(cls, rxn: str | bytes) -> Reaction:
        return _from_constructor(cls, _native.parse(_data(rxn), "rxn"))


def _rdkit() -> Any:
    try:
        return importlib.import_module("rdkit.Chem")
    except ImportError as error:
        raise ImportError("RDKit conversion requires the optional 'rdkit' package") from error


def parse(data: str | bytes, format: str | None = None) -> Drawing:
    """Parse text or bytes, detecting the input format when omitted."""
    return _from_native(_native.parse(_data(data), _format(format) if format else "auto"))


def _drawing(value: Drawing | str | bytes | PathLike[str]) -> Drawing:
    if isinstance(value, Drawing):
        return value
    if isinstance(value, PathLike):
        return Drawing.open(value)
    return parse(value)


def render(
    input: Drawing | str | bytes | PathLike[str], format: str = "png", **options: Any
) -> bytes | str:
    """Parse and render a structure in one call."""
    return _drawing(input).render(format, **options)


def convert(
    input: Drawing | str | bytes | PathLike[str], *, to: str, **options: Any
) -> bytes | str:
    """Convert a structure to a supported output format."""
    return _drawing(input).render(to, **options)


def properties(input: Drawing | str | bytes | PathLike[str]) -> Properties:
    """Parse a structure and calculate its molecular descriptors."""
    return _drawing(input).properties()


class RenderResults(list[bytes | str | None]):
    """Batch results with ``(row_index, message)`` entries in errors and warnings."""

    errors: list[tuple[int, str]]
    warnings: list[tuple[int, str]]

    def __init__(
        self,
        values: Iterable[bytes | str | None],
        errors: list[tuple[int, str]],
        warnings: list[tuple[int, str]] | None = None,
    ) -> None:
        super().__init__(values)
        self.errors = errors
        self.warnings = [] if warnings is None else warnings


def render_many(
    inputs: Iterable[Drawing | str | bytes | PathLike[str]],
    format: str = "png",
    *,
    errors: Literal["none", "raise"] = "none",
    style: str | Style | None = None,
    width: int | None = None,
    height: int | None = None,
    dpi: float | None = None,
    text: Literal["outline", "live"] = "outline",
    **overrides: Any,
) -> RenderResults:
    """Render rows in parallel; bad rows are ``None`` unless ``errors='raise'``.

    Error and warning indexes are zero based and retain the original input order.
    Import warnings are collected in the result instead of emitted individually.
    Configuration errors (such as invalid dimensions or styles) always raise.
    SVG text is drawn as outlines; text='live' keeps <text> elements.
    """
    if errors not in {"none", "raise"}:
        raise ValueError("errors must be 'none' or 'raise'")
    name = _output_format(format)
    _size(width, height, dpi)
    style_json = _style_json(style, overrides)
    items: list[tuple[bytes, str]] = []
    early_errors: list[tuple[int, str]] = []
    early_warnings: list[tuple[int, str]] = []
    for index, value in enumerate(inputs):
        try:
            if isinstance(value, Drawing):
                items.append((value._data, "rsk"))
            elif isinstance(value, PathLike):
                data, _, messages = _read_path(value)
                early_warnings.extend((index, message) for message in messages)
                items.append((data, "rsk"))
            else:
                items.append((_data(value), "auto"))
        except (TypeError, ValueError, OSError) as error:
            early_errors.append((index, str(error)))
            # Keep indexes stable without parsing invalid Python inputs natively.
            items.append((b"", "auto"))
    values, failures, messages = _native.render_many(
        items, name, style_json, width, height, dpi, svg_text=text
    )
    merged = dict(failures)
    merged.update(early_errors)
    failures = sorted(merged.items())
    for index, _ in failures:
        values[index] = None
    if failures and errors == "raise":
        index, message = failures[0]
        raise ValueError(f"Row {index}: {message}")
    return RenderResults(
        (
            value.decode("utf-8") if value is not None and name in _TEXT_FORMATS else value
            for value in values
        ),
        failures,
        sorted(early_warnings + messages, key=lambda warning: warning[0]),
    )


@dataclass(frozen=True)
class _DrawingConstructors(Generic[_D]):
    _drawing_type: type[_D]
    _style: Style

    def open(self, path: str | PathLike[str], *, format: str | None = None) -> _D:
        return self._drawing_type.open(path, format=format).with_style(self._style)

    def from_smiles(self, smiles: str) -> _D:
        return self._drawing_type.from_smiles(smiles).with_style(self._style)

    def from_molfile(self, molfile: str | bytes) -> _D:
        return self._drawing_type.from_molfile(molfile).with_style(self._style)

    def from_inchi(self, inchi: str) -> _D:
        return self._drawing_type.from_inchi(inchi).with_style(self._style)

    def from_cdxml(self, cdxml: str | bytes) -> _D:
        return self._drawing_type.from_cdxml(cdxml).with_style(self._style)

    def from_name(self, name: str, *, online: bool = False) -> _D:
        return self._drawing_type.from_name(name, online=online).with_style(self._style)


class _MoleculeConstructors(_DrawingConstructors[Molecule]):
    def from_rdkit(self, molecule: Any) -> Molecule:
        return self._drawing_type.from_rdkit(molecule).with_style(self._style)


class _ReactionConstructors(_DrawingConstructors[Reaction]):
    def from_rxn(self, rxn: str | bytes) -> Reaction:
        return self._drawing_type.from_rxn(rxn).with_style(self._style)


_DrawingType: TypeAlias = Drawing


@dataclass(frozen=True, init=False)
class StyleContext:
    """Reusable style settings for constructors and functional exports.

    Create one with :func:`with_style`, then use, for example,
    ``custom.Molecule.from_smiles("CCO")`` or ``custom.render("CCO", "svg")``.
    Returned drawings retain the normal :class:`Molecule`, :class:`Reaction`
    and :class:`Drawing` types.
    """

    _style: Style = field(repr=False)

    def __init__(self, style: str | Style | None = None, **overrides: Any) -> None:
        if style is None or isinstance(style, str):
            resolved = Style("JACS / ACS" if style is None else style, **overrides)
        else:
            values = json.loads(cast(str, _style_json(style, overrides)))
            resolved = Style(**values)
        object.__setattr__(self, "_style", resolved)

    @property
    def style(self) -> Style:
        """The validated style shared by this context's operations."""
        return self._style

    @property
    def Drawing(self) -> _DrawingConstructors[_DrawingType]:
        return _DrawingConstructors(Drawing, self._style)

    @property
    def Molecule(self) -> _MoleculeConstructors:
        return _MoleculeConstructors(Molecule, self._style)

    @property
    def Reaction(self) -> _ReactionConstructors:
        return _ReactionConstructors(Reaction, self._style)

    def with_style(self, style: str | Style | None = None, **overrides: Any) -> StyleContext:
        """Derive a context; keyword-only changes keep this context's settings.

        An explicit style name or :class:`Style` selects a new base. Keyword
        overrides are applied last, without changing this context.
        """
        return StyleContext(self._style if style is None else style, **overrides)

    def parse(self, data: str | bytes, format: str | None = None) -> _DrawingType:
        return parse(data, format).with_style(self._style)

    def render(
        self,
        input: _DrawingType | str | bytes | PathLike[str],
        format: str = "png",
        *,
        style: str | Style | None = None,
        **options: Any,
    ) -> bytes | str:
        return render(input, format, style=self._style if style is None else style, **options)

    def convert(
        self,
        input: _DrawingType | str | bytes | PathLike[str],
        *,
        to: str,
        style: str | Style | None = None,
        **options: Any,
    ) -> bytes | str:
        return convert(input, to=to, style=self._style if style is None else style, **options)

    def render_many(
        self,
        inputs: Iterable[_DrawingType | str | bytes | PathLike[str]],
        format: str = "png",
        *,
        errors: Literal["none", "raise"] = "none",
        style: str | Style | None = None,
        width: int | None = None,
        height: int | None = None,
        dpi: float | None = None,
        **overrides: Any,
    ) -> RenderResults:
        return render_many(
            inputs,
            format,
            errors=errors,
            style=self._style if style is None else style,
            width=width,
            height=height,
            dpi=dpi,
            **overrides,
        )


def with_style(style: str | Style | None = None, **overrides: Any) -> StyleContext:
    """Bind a named style and/or overrides to reusable constructors and exports.

    For example, ``custom = reshiki.with_style(font_size=12)`` followed by
    ``custom.Molecule.from_smiles("CCO")`` applies that style at construction.
    Module defaults and existing drawings are unchanged.
    """
    return StyleContext(style, **overrides)


def __getattr__(name: str) -> ModuleType:
    if name == "excel":
        module = importlib.import_module(".excel", __name__)
        globals()[name] = module
        return module
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
