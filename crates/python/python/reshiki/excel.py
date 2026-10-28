"""Lazy Excel integration; install ``reshiki[excel]`` to enable it."""

from __future__ import annotations

import importlib
from io import BytesIO
from os import PathLike
from pathlib import Path
from typing import Any


def add_structures(
    file: str | PathLike[str],
    *,
    smiles: str = "A",
    into: str = "B",
    out: str | PathLike[str] | None = None,
    sheet: str | None = None,
    width: int = 300,
    height: int | None = None,
    header: bool = True,
    start_row: int | None = None,
    **options: Any,
) -> Path:
    """Embed drawings beside nonempty structure cells and save a workbook.

    ``smiles`` and ``into`` are Excel column letters. By default row 1 is a
    header and all nonempty cells from row 2 are rendered. Set ``header=False``
    to start at row 1, or ``start_row`` to choose an explicit first row. Blank
    and invalid rows receive no picture. ``out`` defaults to updating the source file.
    """
    try:
        openpyxl = importlib.import_module("openpyxl")
        image_module = importlib.import_module("openpyxl.drawing.image")
        importlib.import_module("PIL.Image")
        utils = importlib.import_module("openpyxl.utils.cell")
    except ImportError as error:
        raise ImportError(
            "Excel integration requires 'reshiki[excel]' (openpyxl and Pillow)"
        ) from error

    from . import render_many

    if not isinstance(header, bool):
        raise ValueError("header must be a boolean")
    if start_row is None:
        start_row = 2 if header else 1
    if isinstance(start_row, bool) or not isinstance(start_row, int) or start_row < 1:
        raise ValueError("start_row must be a positive integer")
    source_column = utils.column_index_from_string(smiles)
    utils.column_index_from_string(into)
    source = Path(file)
    destination = Path(out) if out is not None else source
    workbook = openpyxl.load_workbook(source, keep_vba=source.suffix.lower() == ".xlsm")
    worksheet = workbook[sheet] if sheet is not None else workbook.active
    if worksheet is None:
        raise ValueError("The workbook has no active worksheet")
    rows: list[int] = []
    structures: list[Any] = []
    for row in range(start_row, worksheet.max_row + 1):
        value = worksheet.cell(row, source_column).value
        if value is not None and str(value).strip():
            rows.append(row)
            structures.append(value)
    images = render_many(structures, width=width, height=height, **options)
    for row, png in zip(rows, images):
        if not isinstance(png, bytes):
            continue
        picture = image_module.Image(BytesIO(png))
        worksheet.add_image(picture, f"{into.upper()}{row}")
        worksheet.row_dimensions[row].height = max(
            worksheet.row_dimensions[row].height or 0, picture.height * 0.75
        )
    worksheet.column_dimensions[into.upper()].width = max(
        worksheet.column_dimensions[into.upper()].width or 0, (width - 5) / 7
    )
    workbook.save(destination)
    workbook.close()
    return destination
