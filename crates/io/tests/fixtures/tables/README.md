The two `native.chemdraw` fixtures are original ReShiki test drawings, authored
for this table implementation and then opened and saved in ChemDrawSDK
26.8.0.6801 on 21 November 2026. They are not copied from the vendor example
corpus.

The six cells contain a benzene ring, captions extending beyond the cell and
table bounds, and competing red/blue and green/purple coincident borders. The
ChemDraw-saved XML records native generated `LineStarts` without inserting
authored line breaks. The matching binary preserves the native `0x8016` table
and `0x801a` border objects. These files exercise the ordinary import route as
well as editable table export.
