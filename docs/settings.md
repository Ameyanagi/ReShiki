# Settings

**Settings** gathers ReShiki's app-wide preferences on one page. They apply to
every drawing and save as you change them. The controls you already know, such
as **View → Smart guides** or **Include editable drawing** on the **Export**
tab, stay where they are and change the same preference.

## Use it

1. Open **Settings** with **Cmd+,** on macOS or **Ctrl+,** on Windows and
   Linux. You can also click the gear button at the right of the toolbar, or
   run **Settings** from the command palette.
2. Choose a section on the left, or type in **Search settings** to list every
   setting whose name or description contains the words.
3. Change a setting. It saves at once.
4. Press **Esc**, press **Cmd+,** or **Ctrl+,** again, or click **Done**. The
   keyboard goes back to the drawing.

| Section        | Settings                                                                                                                                                                                                               |
| -------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Appearance     | **Interface** (**Match canvas**, **Light**, **Dark**) and **Canvas for new drawings**. Each drawing keeps its own canvas once made.                                                                                    |
| Drawing        | **Smart guides**, **Arrange controls** and **Drawing Style units** (pt, mm, cm).                                                                                                                                       |
| Copy & export  | The background of **Copied pictures** and **Exported files**, **Your colors**, **Put the editable drawing in pictures** and **Keep text editable in SVG files**. See [Exporting figures](figure-export.md#background). |
| Names & checks | **Chemical name typography**, **Molecular weight** (3 or 2 decimals), **IUPAC style notes in Check drawing** and **Write Ξ/ξ for unknown configuration**.                                                              |
| Assistant      | **Model**, **Reasoning** and **Accept all edits**. The model and reasoning lists fill in once the assistant connects.                                                                                                  |
| Updates        | **Check for updates automatically** and **Channel** (**Stable** or **Nightly**). Changing the channel checks for that channel's latest release.                                                                        |

To add a background color, click **+** under **Your colors**, type the color as
`#1F4E79`, `31, 78, 121` or `oklch(0.42 0.09 250)`, and click **Add**. The new
color becomes the **Custom** color. Click a saved color to use it as **Custom**.
ReShiki keeps the eight newest colors.

## Limits

- The browser editor has no **Assistant** or **Updates** section: it has no
  local assistant and updates with its page.
- **Esc** closes the whole page, also while you type a color or search.
- Settings changes nothing in a drawing that is already open, except where the
  preference itself does, such as **Interface**.

## Implementation notes

- The page is `src/app/settings_page.rs`. Each row sends the same message as
  its existing control, so saving and error reporting stay in one place.
- The appearance preferences and the new background keys live in
  `appearance.json` in ReShiki's data folder: `copy_background`,
  `export_background` (`transparent`, `canvas`, `white`, `black` or `custom`),
  `custom_background` and `background_colors`. A file from an older version
  loads with copies transparent and exports on the canvas. A file this version
  cannot read is kept as `appearance.json.bad` before a change replaces it.
- Assistant preferences stay in `assistant-preferences.json`, and update
  preferences in `update-preferences.json` and `update-channel.json`.
