# Writing and terms

These rules apply to the user guide (`website/src/content/docs/guide/`), the
feature pages in `docs/*.md` and the release notes. [App text](#app-text) adds
the rules for the strings the app shows.

## Sentences

- Write short, plain, active sentences. Say what the reader does and what
  happens: "Choose **Apply**. Undo restores the previous setting."
- One idea per sentence. Split a sentence that needs a semicolon or more than
  one "while".
- Use the words a chemist uses. Avoid internal terms such as owner, admission,
  producer, qualified, proposal, payload, native or the name of a library.
- Write `.rsk file` for ReShiki's own file format, not "native".

## Labels

- Write each control in bold, exactly as the app shows it, including case and
  the trailing `…`: **Predict NMR…**, **Expand generic structures**.
- Copy labels from the code (the view builders under `src/app/`), not from an
  older page, a review record or a screenshot.
- Write menu paths with arrows: **Properties → 3D appearance → Rear opacity**.
- When a label changes, update every page that quotes it in the same commit.
  `git grep` the old label in `docs/` and `website/src/content/docs/`.

## Feature pages

Each feature page in `docs/` has the same shape:

1. A title and one or two sentences that say what the feature does.
2. `## Use it`: numbered steps with bold labels.
3. `## Limits`: plain bullets on what the feature does not do.
4. `## Implementation notes`: formats, budgets, algorithms and sources for
   developers.

Copy every fact from the code. Keep headings that other pages link to, or
update every link in the same commit (`git grep` the anchor).

## Status and reviews

- Pages that describe a feature say what it does today. They do not mention
  pull requests, review status, test counts or CI runs.
- A feature that is only in Nightly builds carries the line "Available in
  Nightly builds; not in ReShiki 0.15.0."
- Review evidence, screenshots of review sessions and test results belong in
  the records under `docs/changes/`. Release notes link to them.

## One name per feature

| Use                     | Not                                                     |
| ----------------------- | ------------------------------------------------------- |
| Rear opacity            | rear-side opacity, rear visibility, cage opacity        |
| Chemical names          | local Rust chemical naming, OPSIN backend, local naming |
| NMR prediction          | NMR estimate, predicted spectrum                        |
| Generic structures      | Generic Structures, generic expansion feature           |
| Atom maps               | atom mapping proposal, map classes (in user text)       |
| Pen path                | tunable pen line, connected path                        |
| Reference align/stretch | exact reference geometry                                |
| Coordinate in place     | coordination attachment, Coordinate…                    |
| `.rsk` file             | native file, native format, ReShiki native              |

## App text

The labels, tooltips, hints and status messages in `src/` follow the same
rules, plus these:

| Text            | Limit         | Shape                                                                        |
| --------------- | ------------- | ---------------------------------------------------------------------------- |
| Label           | 30 characters | The command's name in sentence case: **Align middle**, **Page setup…**       |
| Tooltip         | 80 characters | The name and its key: **Straight chain ⇧X**. Steps belong in the hint.       |
| Status message  | 60 characters | The result: "3D projection applied". Undo is always there; don't mention it. |
| Section heading | 30 characters | Sentence case in the code. `section()` draws it in capitals.                 |

- End a label with `…` when the command asks for more before it acts.
- Use one name for a command on every surface: toolbar, context row, right-click
  menu, command palette, Properties and Help.
- Write US English: center, color, canceled, stereocenter, symmetrize.
- Write real plurals: "1 template", "3 templates", never "template(s)".
- Name keys as their keycaps: Esc, Enter, Tab, Delete, ⇧. Take shortcuts from
  `shortcuts::keys`, which writes ⌘/⌥ on macOS and Ctrl/Alt elsewhere.
- A label is also the control's accessibility name. In tests, find a control by
  its accessibility id, not its label, so a label change needs no test change.

| Use                         | Not                                                    |
| --------------------------- | ------------------------------------------------------ |
| drawing                     | document, canvas (for the user's work)                 |
| Align left / center / right | Align left edges, Align horizontal centers             |
| Align top / middle / bottom | Align top edges, Align middles, Align vertical centers |
| Flip horizontal / vertical  | Flip horizontally, Flip vertically                     |
| Page setup…                 | Edit page setup…, Set up pages…                        |
| Recalculate                 | Check structure (Check drawing is a different command) |
| Save as .rsk…               | Save native document…                                  |
| Abbreviate selection…       | Create group label…, Contract selection                |
| Save as abbreviation        | Define abbreviation from selection                     |
| 3D optimize…                | 3D optimization                                        |
| Clean up…                   | Clean up structure, Tidy                               |
| Atom maps                   | Atom mapping                                           |
| Show/hide side panel        | Toggle inspector, Show or hide inspector               |
| Assistant                   | AI assistant, Codex (for the panel)                    |

`scripts/copy_lint.py` checks the rules it can: spelling, `(s)` plurals,
"Escape" in text, Undo tails, capitals passed to `section()` and the status
message length. The `web` job in Checks runs it; run it locally with
`python3 scripts/copy_lint.py`.
