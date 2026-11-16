# Authored native geometry during passive operations

`unsettled-native.rsk` is a hand-authored synthetic native drawing. It is valid
model data with intentionally unreconciled presentation: an attached quadratic
mechanism arrow starts inside normal label clearance; an oversized linked
condition caption sits far from its short arrow; a linked molecule name and
property caption are manually positioned; a free legacy fishhook has no stored
control; and a rounded rectangle has a skewed coordinate frame.

Open, copy, analysis, printing, and export snapshots must keep the stored
geometry and links. Pasting may remap IDs and apply one common translation.
An actual edit or explicit Clean Up still runs linked constraint reconciliation.
The native links have no complete CDXML/CDS representation; the separate
ChemDraw clipboard-layout controls cover imported external drawing geometry.
No third-party artwork or production exporter constructs this input.

`remote-property-caption.cdxml` is a separate hand-authored external control.
Its active formula link intentionally has remote, author-written text. Import
retains that text and its coordinates; the next chemistry edit recalculates the
linked formula. The paired full-drawing `.cds` is independently encoded using
`reference/engine/cdx_exchange.py`, with packed default text style properties.
It is synthetic data, not a file saved by native ChemDraw or ReShiki.
