---
section: Changes
issues: []
---

- **Settings** gathers the app-wide preferences on one page: appearance,
  drawing, copy and export, names and checks, the assistant and updates. Open it
  with **Cmd+,** (**Ctrl+,** on Windows and Linux), the gear button at the right
  of the toolbar, or **Settings** in the command palette. **Search settings**
  finds a setting by name. Each change saves at once, and the controls you use
  today stay where they are. See [Settings](settings.md).
- Copied pictures and exported files can have a **Transparent**, **Canvas**,
  **White**, **Black** or **Custom** background, with separate defaults for each:
  copies stay transparent and exports keep the canvas unless you change them.
  On white, black or a color, black and white ink swap so a dark drawing stays
  readable. Every other color stays as it is. Save your own colors for
  **Custom** in **Settings → Copy & export**. Custom is one color for copies
  and exported files. The **Export** tab has the same
  **Background** choice, and the right-click menu has **Copy image on** for one
  copy. See [Exporting figures](figure-export.md#background).
- **Settings → Copy & export** previews the drawing as copied and as exported,
  each on its background. The **Export** tab previews the file it will write,
  above the **Export** button.
- Copied pictures and exported files each have a PNG resolution of 150, 300,
  600 or 1200 dpi. Copies now default to 300 dpi, which pastes faster and
  smaller. Exported files stay at 1200 dpi. The size in centimeters is the same
  at every setting. **Settings → Copy & export** shows close-ups of the drawing
  at each resolution, and the **Export** tab has a **Resolution** menu with the
  drawing's size in centimeters and pixels. See
  [PNG resolution](figure-export.md#png-resolution).
- **Put the editable drawing in pictures** in **Settings → Copy & export** now
  also applies to **Copy**, **Copy image** and **Copy as**. With it off, copied
  pictures carry no drawing, and on Windows **Copy** pastes into Office as a
  picture. Pasting between ReShiki windows still keeps the drawing, except in
  the browser editor. **Editable drawing** in **Copy image on** changes it for
  one copy, and **Include editable drawing** on the **Export** tab for that
  tab's exports, with **Restore default** to go back.
- Each **Settings** section has **Restore defaults**, which puts only that
  section back as ReShiki ships it. Your saved colors stay.
- A PNG's editable drawing now sits after the image data. PNG files from older
  versions still open as drawings.
