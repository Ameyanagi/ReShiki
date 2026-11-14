"""List the test fixture rows a pull request changes, for the job summary.

Usage: baseline_summary.py BASE [HEAD]. Text fixtures (gzipped ones too) are
compared row by row; `.tsv` rows are matched by their first column. The output
is Markdown, appended to $GITHUB_STEP_SUMMARY when that is set.
"""

import gzip
import os
import subprocess
import sys
from collections import Counter
from pathlib import Path

FIXTURES = ":(glob)**/tests/fixtures/**"
ROW_LIMIT = 40  # rows shown per file
ROW_WIDTH = 300  # characters shown per row


def row_changes(old, new, keyed):
    old, new = old.splitlines(), new.splitlines()
    if not keyed:
        removed, added = Counter(old) - Counter(new), Counter(new) - Counter(old)
        return [f"- {row}" for row in removed.elements()] + [f"+ {row}" for row in added.elements()]
    before = {row.split("\t", 1)[0]: row for row in old}
    after = {row.split("\t", 1)[0]: row for row in new}
    changes = []
    for key, row in before.items():
        if after.get(key) != row:
            changes.append(f"- {row}")
            if key in after:
                changes.append(f"+ {after[key]}")
    return changes + [f"+ {row}" for key, row in after.items() if key not in before]


def content(root, revision, path):
    result = subprocess.run(
        ["git", "-C", str(root), "show", f"{revision}:{path}"], capture_output=True, check=False
    )
    # A file that is absent on one side compares as empty.
    data = result.stdout if result.returncode == 0 else b""
    if path.endswith(".gz") and data:
        data = gzip.decompress(data)
    return data


def summary(base, head="HEAD", root="."):
    paths = subprocess.check_output(
        ["git", "-C", str(root), "diff", "--no-renames", "--name-only", "-z", base, head]
        + ["--", FIXTURES]
    )
    paths = [path for path in paths.decode("utf-8", errors="surrogateescape").split("\0") if path]
    if not paths:
        return ""
    lines = [f"### {len(paths)} changed fixture files", ""]
    for path in paths:
        old, new = content(root, base, path), content(root, head, path)
        try:
            if b"\0" in old + new:
                raise ValueError
            old, new = old.decode("utf-8"), new.decode("utf-8")
        except ValueError:
            lines += [f"- `{path}`: binary, {len(old)} to {len(new)} bytes", ""]
            continue
        changes = row_changes(old, new, keyed=path.endswith(".tsv"))
        removed = sum(row.startswith("-") for row in changes)
        lines += [f"- `{path}`: {removed} removed, {len(changes) - removed} added", ""]
        shown = [row[:ROW_WIDTH] for row in changes[:ROW_LIMIT]]
        if len(changes) > ROW_LIMIT:
            shown.append(f"... {len(changes) - ROW_LIMIT} more rows")
        # Every row starts with "- ", "+ " or "...", so none can close the block.
        lines += ["```diff", *shown, "```", ""]
    return "\n".join(lines)


def main():
    text = summary(*sys.argv[1:3])
    target = os.environ.get("GITHUB_STEP_SUMMARY")
    if target and text:
        with Path(target).open("a", encoding="utf-8") as output:
            # GitHub rejects a step summary over 1 MiB.
            output.write(text[:900_000])
    print(text or "No fixture files changed.")


if __name__ == "__main__":
    main()
