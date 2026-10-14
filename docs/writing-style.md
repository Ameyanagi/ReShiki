# Writing and terms

These rules apply to the user guide (`website/src/content/docs/guide/`), the
feature pages in `docs/*.md` and the release notes.

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
  Nightly builds; not in ReShiki 0.12.0."
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
