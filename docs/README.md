# Documentation tree

This folder holds ReShiki's user and developer documentation. Follow these rules
when you add a page, a review record or an image.

- **Pages.** Every other `docs/*.md` file is a user or developer page that the
  website publishes, so link it from the sidebar, a guide or another page. Put
  only Markdown files at the top level of `docs/`.
- **Review records.** Each change gets one record at `docs/changes/<topic>.md`.
  Say what was checked and what the result was. Use at most three images.
- **Images.** Put screenshots in `docs/images/<topic>/`. Every file there must be
  linked from a page, a record or the website.
- **Test inputs.** Put files that tests read in `tests/fixtures/<topic>/`. Code never
  reads non-Markdown files from `docs/`. Generated pages with a drift test, such as
  `docs/agent-reference.md`, are the only files under `docs/` that code reads.
- **Machine receipts.** Provenance and audit JSON, logs, hashes, verification
  scripts and benchmark run output stay out of the main branch. Attach them to the
  pull request, or add them to an evidence branch such as
  [`evidence/review-archive-2026-10`](https://github.com/Ameyanagi/ReShiki/tree/evidence/review-archive-2026-10),
  and link them from the record.

`bun scripts/check_docs_tree.mjs` checks the image, top-level and code rules.
`bun run docs:check` runs it together with the website checks.
