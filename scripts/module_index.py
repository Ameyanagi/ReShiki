"""Write docs/modules.md: every Rust module file with the first sentence of its `//!` comment.

Usage: module_index.py [--check]. Without --check the page is rewritten; with
it the script exits 1 when the page differs from the source tree. Test modules
(`tests.rs`, `*_tests.rs` and files under a `tests/` folder) are left out.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PAGE = ROOT / "docs/modules.md"
TEST_PART = re.compile(r"^(tests?|.*_tests?)(\.rs)?$")
HEAD = """# Module index

<!-- Generated: python3 scripts/module_index.py -->

Every Rust module file of the workspace with the first sentence of its `//!` comment.
A file without that comment has no description yet. [Architecture](architecture.md)
explains how the crates and modules fit together.
"""


def summary(path):
    """The first sentence of the file's leading `//!` comment, or an empty string."""
    words = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.startswith("//!"):
            if line.startswith(("#![", "//")) or not line.strip():
                continue
            break
        text = line[3:].strip()
        if not text and words:
            break
        words.append(text)
        if re.search(r"[.:]$", text):
            break
    sentence = re.split(r"(?<=[.:])\s", " ".join(words).strip(), maxsplit=1)[0].rstrip(":")
    # Rustdoc links become plain code; `<` and `*` outside code would be read as Markdown.
    sentence = re.sub(r"\[(`[^`]+`)\](\([^)]*\))?", r"\1", sentence)
    parts = sentence.split("`")
    parts[::2] = [re.sub(r"([<*])", r"\\\1", part) for part in parts[::2]]
    return "`".join(parts)


def crates():
    for manifest in [ROOT / "Cargo.toml"] + sorted(ROOT.glob("*/*/Cargo.toml")):
        source = manifest.parent / "src"
        name = re.search(r'^name = "(.+)"$', manifest.read_text(encoding="utf-8"), re.M)
        # Disposable validation checkouts under the ignored artifact root are
        # output, just like vendored code, rather than repository modules.
        if source.is_dir() and name and manifest.parent.parent.name not in {"artifacts", "vendor"}:
            yield name.group(1), source


def page():
    lines = [HEAD]
    for name, source in crates():
        lines.append(f"## {name}\n")
        for path in sorted(source.rglob("*.rs"), key=Path.as_posix):
            relative = path.relative_to(ROOT)
            if any(TEST_PART.match(part) for part in relative.parts):
                continue
            text = summary(path)
            lines.append(f"- `{relative.as_posix()}`" + (f": {text}" if text else ""))
        lines.append("")
    return "\n".join(lines)


def main():
    if sys.argv[1:] == ["--check"]:
        if PAGE.read_text(encoding="utf-8") != page():
            sys.exit("docs/modules.md is out of date: run python3 scripts/module_index.py")
    else:
        PAGE.write_bytes(page().encode("utf-8"))


if __name__ == "__main__":
    main()
