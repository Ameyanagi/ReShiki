# Unreleased change fragments

Each pull request with a change worth a release note adds one file here, named
after its branch with `/` replaced by `-`. For branch `fix/paste-bond`, add
`fix-paste-bond.md`:

```md
---
section: Fixes
issues: [1151]
credit: [@contributor]
---

- One bullet per user-visible change, in the style of the release notes.
```

- `section` is **Changes**, **Fixes** or **Developer**.
- `issues` lists the issue numbers the change fixes, or `[]`.
- `credit` is optional. It lists the GitHub accounts of outside contributors to
  credit in the release notes and [the contributor list](../../contributors.md).
- Write links relative to `docs/`, such as `[Clipboard](clipboard.md)`: the
  bullets are shown on the [unreleased changes](../../changes-unreleased.md)
  page there.

A pull request never edits `docs/changes-unreleased.md` or
`docs/contributors.md`, so fragments do not conflict. Check them with:

```sh
uv run --locked python scripts/assemble_changelog.py --check
```

Print the assembled page, as the website shows it, with
`uv run --locked python scripts/assemble_changelog.py`. Release preparation moves
the fragments into the versioned notes; see
[publish a version](../../releasing.md#publish-a-version).
