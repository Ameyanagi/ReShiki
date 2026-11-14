# Contributing to ReShiki

## License agreement

By intentionally submitting an original contribution for inclusion in ReShiki,
you agree to make that contribution available under **both** the
[MIT License](LICENSE-MIT) and the [Apache License, Version 2.0](LICENSE-APACHE),
so recipients may choose either license (`MIT OR Apache-2.0`). This applies to
original code, tests, and documentation. You retain your copyright; this is
not a copyright assignment.

Submit only work that you have the right to contribute under these terms,
including any necessary employer or other rights-holder authorization.

For third-party code or data that you copy, port, adapt, or bundle, identify
the source, version, and applicable license in the pull request. Preserve
the original copyright notices, license texts, and required attribution.
Do not represent third-party material as your original dual-licensed work.
Changes to separately licensed components must comply with their existing
terms. See [LICENSE](LICENSE) and [NOTICE](NOTICE) for the project's scope
and existing attributions.

Confirm the contribution license agreement in the pull request checklist.
The contribution license check must pass before merging.
If you cannot agree, explain the licensing issue before submitting code;
the contribution cannot be merged under the default terms until it is resolved.

## Development

Open feature and fix pull requests against **`main`**, the default and development
branch. Nightly prereleases are built from `main`. Stable release tags identify
tested commits already on `main`.

See the [developer guide](https://reshiki.com/developer/development/) for setup
and checks. Describe the problem your change solves and the relevant validation
in your pull request.

Ordinary pull requests run CI on Linux and Windows only. The macOS legs run on
stacked `merge/*` pull requests, on pull requests labelled `full-ci`, and on
`main`, nightly and release builds. CI reads the label when it starts, so add
`full-ci` before the next push, or close and reopen the pull request.

Include [visual review evidence](docs/visual-review.md): example images for new
features, labeled examples for each new shortcut, and matched before/after
images for visible bug fixes. Provide a reusable caption and image link for the
release changelog. Explain the evidence used for changes with no visible output.

## Release notes

Add one file for your change to [`docs/changes/unreleased/`](docs/changes/unreleased/README.md),
named after your branch with `/` replaced by `-`. It names the section and the
issues it fixes, and holds one bullet per user-visible change. Do not edit
`docs/changes-unreleased.md` or `docs/contributors.md`: the website assembles
the unreleased page from the fragments, and release preparation moves them
into the release notes and the contributor list.

## Contributor credit

When preparing a contribution for merge, preserve the original author's commit
attribution. If changes are squashed together with maintainer work, retain the
original author or add an accurate `Co-authored-by` trailer using the author's
existing commit identity.

Credit an outside contributor with `credit: [@name]` in the change's
[release-note fragment](docs/changes/unreleased/README.md). Release preparation
adds that credit beside the change in the release notes and to
[the contributor list](docs/contributors.md).

Keep @Ameyanagi's contributor-list entry as "project creator and maintainer";
credit individual changes in the release notes.
