# Settings

**Settings** gathers ReShiki's app-wide preferences on one page. They apply to
every drawing and save as you change them. The controls you already know, such
as **View → Smart guides** or **Background** on the **Export** tab, stay where
they are and change the same preference.

## Use it

1. Open **Settings** with **Cmd+,** on macOS or **Ctrl+,** on Windows and
   Linux. You can also click the gear button at the right of the toolbar, or
   run **Settings** from the command palette.
2. Choose a section on the left, or type in **Search settings** to list every
   setting whose name or description contains the words.
3. Change a setting. It saves at once. If it cannot be saved, its section says
   why.
4. To put a section back as ReShiki ships it, click **Restore defaults** at the
   bottom of the section. The status bar says, for example, **Copy & export
   restored to defaults**. The button is dimmed when the section already has
   its defaults.
5. Press **Esc**, press **Cmd+,** or **Ctrl+,** again, or click **Done**. The
   keyboard goes back to the drawing.

| Section        | Settings                                                                                                                                                                                                                                                                                                                                    |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Appearance     | **Interface** (**Match canvas**, **Light**, **Dark**) and **Canvas for new drawings**. Each drawing keeps its own canvas once made.                                                                                                                                                                                                         |
| Drawing        | **Smart guides**, **Arrange controls**, **Drawing Style units** (pt, mm, cm), and separate **Ruler units** (mm, cm, in, pt).                                                                                                                                                                                                                |
| Copy & export  | The background of **Copied pictures** and **Exported files** with a **Preview** of each, **Your colors**, the **PNG resolution** of each (150, 300, 600 or 1200 dpi) with close-ups of the drawing, **Put the editable drawing in pictures** and **Keep text editable in SVG files**. See [Exporting figures](figure-export.md#background). |
| Names & checks | **Chemical name typography**, **Molecular weight** (3 or 2 decimals), **IUPAC style notes in Check drawing** and **Write Ξ/ξ for unknown configuration**.                                                                                                                                                                                   |
| Assistant      | **Model**, **Reasoning** and **Accept all edits**. The model and reasoning lists fill in once the assistant connects.                                                                                                                                                                                                                       |
| Updates        | **Check for updates automatically** and **Channel** (**Stable** or **Nightly**). Changing the channel checks for that channel's latest release.                                                                                                                                                                                             |

To add a background color, click **+** under **Your colors**. Pick a palette
color, or type the color as `#1F4E79`, `31, 78, 121` or `oklch(0.42 0.09 250)`
and click **Add**. The new color becomes the **Custom** color, and copied
pictures use it. Click a saved color to do the same with it. ReShiki keeps the
eight newest colors. **Custom** is one color for copied pictures and exported
files, so when exported files use **Custom**, they change to the new color too.
On **White**, **Black** or **Custom**, black and white ink swap so a dark
drawing stays readable. Colored atoms and other colors stay as they are, and
**Canvas** keeps the drawing's own colors.

**Preview** shows the current drawing as **Copy** and **Export** write it, each
on its background, with a checkerboard where it is transparent. ReShiki makes
the previews in the background after each change to the drawing or to a
background. An empty drawing shows a sample structure instead.

**Restore defaults** changes only its own section. In **Copy & export** it sets
copied pictures to **Transparent** at 300 dpi and exported files to **Canvas**
at 1200 dpi, and turns **Put the editable drawing in pictures** on and **Keep
text editable in SVG files** off. Your colors and the **Custom** color stay.
There is no Undo, as these are preferences: change a setting again to go back.

Under **PNG resolution**, **Copied pictures** defaults to 300 dpi and
**Exported files** to 1200 dpi. Below them, a close-up of the current drawing at
each resolution shows the pixels the PNG will have, at the same size on screen,
with the whole drawing's pixel size underneath. The size in centimeters stays
the same at every setting. **Copy** and **Export** tags mark
the resolutions in use. The close-ups are made in the background after each
change to the drawing. An empty drawing shows a sample structure instead. See
[PNG resolution](figure-export.md#png-resolution).

## Limits

- The browser editor has no **Assistant** or **Updates** section: it has no
  local assistant and updates with its page.
- While the color picker is open, **Esc** closes only the picker. Otherwise it
  closes the whole page, also while you search.
- **Write Ξ/ξ for unknown configuration** waits while a name is being made, as
  it does on the **Names** panel.
- **Put the editable drawing in pictures** is the default for copies and
  exports. **Editable drawing** in **Copy image on** changes it for one copy,
  and **Include editable drawing** on the **Export** tab for the tab's exports.
  Neither changes the default. See
  [Keep the drawing editable inside a picture](figure-export.md#keep-the-drawing-editable-inside-a-picture)
  for what it puts in pictures and who can read it.
- **Keep text editable in SVG files** applies to exported SVG files. SVG for
  Office and copied SVG always turn text into outlines.
- Settings changes nothing in a drawing that is already open, except where the
  preference itself does, such as **Interface**.

## Implementation notes

- The page is `src/app/settings_page.rs`. Each row sends the same message as
  its existing control, so saving and error reporting stay in one place. The
  color picker shares the swatches and color box of the style bar's color
  popover.
- The appearance preferences and the new background keys live in
  `appearance.json` in ReShiki's data folder: `copy_background`,
  `export_background` (`transparent`, `canvas`, `white`, `black` or `custom`),
  `custom_background` and `background_colors`. The PNG resolutions are
  `copy_dpi` and `export_dpi`, each 150, 300, 600 or 1200. A file from an older
  version loads with copies transparent at 300 dpi and exports on the canvas at
  1200 dpi. A file this version cannot read, such as one with another
  resolution, is kept as `appearance.json.bad` before a change replaces it.
- The drawing style's own PNG resolution stays 1200 dpi, so drawings and
  styles saved by any version still load. The app preference applies only when
  ReShiki writes a PNG.
- Assistant preferences stay in `assistant-preferences.json`, and update
  preferences in `update-preferences.json` and `update-channel.json`.
- **Restore defaults** sets the section's keys to the values a new
  `appearance.json` has, and leaves `custom_background` and
  `background_colors` alone. In **Assistant** it clears the model and the
  reasoning levels and turns **Accept all edits** off. In **Updates** it turns
  automatic checks on and chooses **Stable**, which checks that channel. It is
  disabled while an update is being saved or installed.
