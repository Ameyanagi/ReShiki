---
section: Changes
issues: []
---

- The interface uses the platform's own sans-serif: Helvetica Neue on macOS,
  Segoe UI on Windows and Noto Sans on Linux. A label ending in "…", such as
  **Predict NMR…**, no longer reads as "Predict NMR···". Japanese and other CJK
  text in the interface comes from the system's CJK font, in the glyph forms of
  the first CJK language or region among the preferred languages, so English
  (Japan) keeps Japanese kanji forms. Drawing labels keep their own fonts.
- Before any command has run, the command palette opens on a **Start here**
  group: **Import structure or name**, **Benzene**, **Open…**,
  **Clean up…**, **Molecular formula and properties**,
  **Predict NMR**, **Export panel** and **Help & shortcuts**. It used to open
  on Cyclopropane and the other ring templates.
- Help rows that are also commands, such as **Select all** or **Fit drawing**,
  are marked **Run ↩**. **Enter** or **Run** runs the command and closes Help,
  after a click on the row too; when it cannot run now, Help says why. A
  caption you are typing is applied first, as from the command palette.
- Keyboard focus is drawn as a ring around the control, with a gap, so a
  focused button no longer looks like the selected one.
- Screen readers can reach the document tabs and their close buttons, the
  side panel's tabs, the font size field, the **B**, **I**, **U**, formula,
  subscript and superscript buttons, the font, journal preset and color theme
  lists, and the light/dark canvas button. Tabs say whether they are selected or
  edited. With a journal preset or color theme list focused, **↑** and **↓**
  choose the previous or next option.
- **Tab** reaches the **Font** box: type to search, **↑** and **↓** move through
  the list, and **Enter** picks the font and returns focus to the drawing.
