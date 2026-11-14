"""Assemble the unreleased changes from one fragment file per change.

Each pull request adds docs/changes/unreleased/<branch-slug>.md:

    ---
    section: Fixes
    issues: [1151]
    credit: [@contributor]
    ---
    - One bullet per user-visible change.

`section` is Changes, Fixes or Developer, and `credit` is optional. Write links
relative to docs/, where the assembled page lives.

With no option, print docs/changes-unreleased.md (the hand-kept head) followed
by every fragment, grouped by section; the website shows that page. `--check`
only validates the fragments. `--release VERSION` appends them to the versioned
notes, credits contributors in docs/contributors.md and deletes the fragments.
"""

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DOCS = ROOT / "docs"
FRAGMENTS = DOCS / "changes/unreleased"
SECTIONS = ("Changes", "Fixes", "Developer")
ISSUES = "https://github.com/Ameyanagi/ReShiki/issues/"


def parse_list(value):
    items = value.strip()
    if not (items.startswith("[") and items.endswith("]")):
        raise ValueError(f"expected a [list], got {value!r}")
    return [item.strip().strip("\"'") for item in items[1:-1].split(",") if item.strip()]


def parse(path):
    """Return (section, issues, credit, body) of one fragment."""
    text = path.read_text(encoding="utf-8")
    match = re.fullmatch(r"---\n(.*?)\n---\n(.*)", text, re.DOTALL)
    if not match:
        raise ValueError(f"{path.name}: start with front matter between --- lines")
    fields = {}
    for line in match[1].splitlines():
        key, separator, value = line.partition(":")
        if not separator:
            raise ValueError(f"{path.name}: expected 'key: value', got {line!r}")
        fields[key.strip()] = value.split("#", 1)[0].strip()
    unknown = fields.keys() - {"section", "issues", "credit"}
    if unknown:
        raise ValueError(f"{path.name}: unknown field {', '.join(sorted(unknown))}")
    section = fields.get("section")
    if section not in SECTIONS:
        raise ValueError(f"{path.name}: section must be one of {', '.join(SECTIONS)}")
    if "issues" not in fields:
        raise ValueError(f"{path.name}: add issues: [] when no issue applies")
    try:
        issues = [int(issue.lstrip("#")) for issue in parse_list(fields["issues"])]
        credit = parse_list(fields.get("credit", "[]"))
    except ValueError as error:
        raise ValueError(f"{path.name}: {error}") from None
    if any(not name.startswith("@") for name in credit):
        raise ValueError(f"{path.name}: credit GitHub accounts as @name")
    body = match[2].strip()
    if not body.startswith("- "):
        raise ValueError(f"{path.name}: write the change as '- ' bullets")
    return section, issues, credit, body


def fragments():
    paths = sorted(p for p in FRAGMENTS.glob("*.md") if p.name != "README.md")
    return [(path, *parse(path)) for path in paths]


def entry(issues, credit, body):
    """The fragment's bullets, with its issues and credit after the last one."""
    notes = [f"[#{issue}]({ISSUES}{issue})" for issue in issues]
    if credit:
        notes.append(f"thanks to {', '.join(credit)}")
    return f"{body} ({'; '.join(notes)})" if notes else body


def sections(parsed):
    blocks = []
    for name in SECTIONS:
        entries = [entry(*rest) for _, section, *rest in parsed if section == name]
        if entries:
            blocks.append(f"## {name}\n\n" + "\n".join(entries) + "\n")
    return "\n".join(blocks)


def assemble():
    head = (DOCS / "changes-unreleased.md").read_text(encoding="utf-8").rstrip()
    changes = sections(fragments())
    return f"{head}\n\n{changes}" if changes else f"{head}\n"


def release(version):
    """Fold the fragments into the versioned notes and contributors.md, then delete them."""
    parsed = fragments()
    if not parsed:
        raise ValueError(f"No fragments in {FRAGMENTS.relative_to(ROOT)}")
    # 0.16.0 → changes-0.16.md, as for earlier releases; 0.16.1 → changes-0.16.1.md.
    notes = DOCS / f"changes-{version.removesuffix('.0')}.md"
    existing = notes.read_text(encoding="utf-8").rstrip() if notes.exists() else ""
    notes.write_text(
        f"{existing or f'# ReShiki {version}'}\n\n{sections(parsed)}", encoding="utf-8"
    )
    credits = []
    for _, _, _, credit, body in parsed:
        if credit:
            summary = " ".join(body.split("\n- ", 1)[0].removeprefix("- ").split())
            credits.append(f"{', '.join(credit)} — {summary} In [ReShiki {version}]({notes.name}).")
    if credits:
        contributors = DOCS / "contributors.md"
        text = contributors.read_text(encoding="utf-8").rstrip()
        contributors.write_text(text + "\n\n" + "\n\n".join(credits) + "\n", encoding="utf-8")
    for path, *_ in parsed:
        path.unlink()
    print(f"Moved {len(parsed)} fragments into {notes.relative_to(ROOT)}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--check", action="store_true", help="validate the fragments")
    mode.add_argument("--release", metavar="VERSION", help="fold fragments into release notes")
    args = parser.parse_args()
    try:
        if args.check:
            print(f"{len(fragments())} changelog fragments are valid")
        elif args.release:
            release(args.release)
        else:
            sys.stdout.write(assemble())
    except ValueError as error:
        print(f"Changelog fragment error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
