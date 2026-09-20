# ruviz integration requirements

The molecular editor does not currently require changes to ruviz. Its scene contains atoms, bonds and drawing objects, while ruviz would serve the spectra and analytical plotting workspace.

Inspected the local `~/dev/ruviz` source on 2026-09-16. The existing `ruviz-iced` adapter targets Iced 0.14 and exposes retained `PlotState`, `set_plot_keep_view`, reactive subscriptions, hover/click/selection events, and view-change notifications. The core provides reversed X axes, annotations, coordinate transforms and PDF export. The adapter's built-in export menu saves the presented raster frame as PNG. No ruviz files were changed as part of this work.

Before implementing a spectrum panel, confirm these two end-to-end paths:

1. **Range gestures:** application-visible selection-start/change/end events with data-coordinate endpoints, including cancellation. The spectrum use case needs X-only selection alongside zoom and pan, with reversed ppm axes. ReShiki will calculate areas and perform peak detection; ruviz only needs to report the user's region accurately.
2. **Vector export of the active view:** produce SVG/PDF using the current bounds, axis direction, annotations and labels. It should not require the application to reconstruct the plotted viewport from a raster frame. An off-thread export task fits the current Iced adapter design.

These are integration paths to validate, not claims that the underlying functionality is absent. Existing core session/annotation/export APIs may already provide most of the pieces.

ReShiki should own spectrum data, stable peak IDs, integration calculations and peak-to-atom assignments. It can map ruviz series/sample hits to those IDs. A decreasing X axis for NMR, stacked traces, linked molecular highlighting and CSV import are the first intended workflows. Spectrum plotting is separate from NMR prediction, which requires an independently validated chemistry model.
