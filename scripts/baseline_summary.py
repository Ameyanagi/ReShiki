"""List the test fixture rows a pull request changes, for the job summary.

Usage: baseline_summary.py BASE [HEAD]. Text fixtures (gzipped ones too) are
compared row by row; `.tsv` rows are matched by their leading columns, as many
as tell the rows apart. The output is Markdown, appended to
$GITHUB_STEP_SUMMARY when that is set.
"""

import gzip
import os
import re
import subprocess
import sys
from collections import Counter
from pathlib import Path

FIXTURES = ":(glob)**/tests/fixtures/**"
ROW_LIMIT = 40  # rows shown per file
ROW_WIDTH = 300  # characters shown per row
SUMMARY_BYTES = 900_000  # GitHub rejects a step summary over 1 MiB


def row_changes(old, new, keyed):
    old, new = old.splitlines(), new.splitlines()
    # The key is the fewest leading columns that tell every row apart on both sides.
    width = None
    if keyed:
        columns = max((row.count("\t") + 1 for row in old + new), default=0)
        width = next(
            (
                width
                for width in range(1, columns + 1)
                if all(
                    len({tuple(row.split("\t")[:width]) for row in rows}) == len(rows)
                    for rows in (old, new)
                )
            ),
            None,
        )
    if width is None:
        removed, added = Counter(old) - Counter(new), Counter(new) - Counter(old)
        return [f"- {row}" for row in removed.elements()] + [f"+ {row}" for row in added.elements()]
    before = {tuple(row.split("\t")[:width]): row for row in old}
    after = {tuple(row.split("\t")[:width]): row for row in new}
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


def quoted(path):
    """The path as inline code that no character in it can end."""
    path = "".join(
        char if char.isprintable() else char.encode("unicode_escape").decode("ascii")
        for char in path
    )
    ticks = re.findall("`+", path)
    if not ticks:
        return f"`{path}`"
    fence = "`" * (len(max(ticks, key=len)) + 1)
    return f"{fence} {path} {fence}"


def clip(text, limit=SUMMARY_BYTES):
    data = text.encode("utf-8")
    if len(data) <= limit:
        return text
    # Cut at the end of a line, then close a code block that the cut left open.
    kept = data[:limit].decode("utf-8", errors="ignore")
    kept = kept[: kept.rfind("\n") + 1]
    if sum(line.startswith("```") for line in kept.split("\n")) % 2:
        kept += "```\n"
    return kept + "\nThe rest of the list is too long to show.\n"


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
            lines += [f"- {quoted(path)}: binary, {len(old)} to {len(new)} bytes", ""]
            continue
        changes = row_changes(old, new, keyed=path.endswith(".tsv"))
        removed = sum(row.startswith("-") for row in changes)
        lines += [f"- {quoted(path)}: {removed} removed, {len(changes) - removed} added", ""]
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
            output.write(clip(text))
    print(text or "No fixture files changed.")


if __name__ == "__main__":
    main()
