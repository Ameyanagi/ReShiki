"""Register the Series accessor with ``import reshiki.pandas``.

Install ``reshiki[pandas]`` to enable this optional integration.
"""

from __future__ import annotations

import importlib
from dataclasses import asdict
from typing import Any


def register() -> None:
    try:
        pd = importlib.import_module("pandas")
    except ImportError as error:
        raise ImportError("Pandas integration requires 'reshiki[pandas]'") from error
    if hasattr(pd.Series, "reshiki"):
        return

    @pd.api.extensions.register_series_accessor("reshiki")
    class ReShikiAccessor:
        def __init__(self, series: Any) -> None:
            self._series = series

        def _render(self, format: str, **options: Any) -> Any:
            from . import render_many

            values = render_many(self._series, format=format, **options)
            result = pd.Series(values, index=self._series.index, name=self._series.name)
            result.attrs["reshiki_errors"] = values.errors
            return result

        def png(self, **options: Any) -> Any:
            """Return image bytes, preserving the Series index and name."""
            return self._render("png", **options)

        def svg(self, **options: Any) -> Any:
            return self._render("svg", **options)

        def properties(self, *, errors: str = "none") -> Any:
            """Return a descriptor DataFrame, with missing values for bad rows."""
            from . import properties

            if errors not in {"none", "raise"}:
                raise ValueError("errors must be 'none' or 'raise'")
            rows = []
            failures = []
            for index, value in enumerate(self._series):
                try:
                    rows.append(asdict(properties(value)))
                except (TypeError, ValueError, OSError) as error:
                    if errors == "raise":
                        raise ValueError(f"Row {index}: {error}") from error
                    rows.append({})
                    failures.append((index, str(error)))
            result = pd.DataFrame(rows, index=self._series.index)
            result.attrs["reshiki_errors"] = failures
            return result


register()
