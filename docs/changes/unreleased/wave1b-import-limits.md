---
section: Fixes
issues: []
---

- A CDXML file or ChemDraw style file whose elements are nested thousands of
  levels deep no longer closes ReShiki. A file nested more than 64 levels deep
  is refused, and the message names the limit. Over MCP such a file used to end
  the session.
- A CDXML file with more than 256 attributes on one element is refused at once.
  A 209 KB file with 20,000 attributes used to take 8 seconds to read, and
  larger ones took minutes.
- A CDXML file or ChemDraw style file that declares entities or other DTD
  content inside the file is refused before it is read. A style file used to
  be read first, and a CDXML file could hide an entity behind another
  declaration. A reference to the ChemDraw DTD is still accepted.
- A text box holds at most 64 KB of text, and a drawing at most 1 MB. A file,
  an edit or an MCP change over the limit is refused with a message. A `.rsk`
  file with 200,000 characters in one text box used to take over a minute to
  open, and a million characters used close to 1 GB of memory.
- MCP session documents now count text toward the session weight limit: every
  64 bytes of text weigh as much as one object.
- A PNG is at most 32,767 pixels on a side. Export lowers the resolution to
  stay within it, or says that the drawing is too large for a PNG. One long
  line of text used to export as an image 444,586 pixels wide.
- A `.rsk` file whose pictures hold more than 64 million pixels in total is
  refused when it is opened, before any picture is decoded. It used to be
  refused only after every picture was decoded, which took up to a minute.
- PNG export of a drawing that is too large even at 72 dpi is refused before
  the drawing is checked and rendered, not after.
