#!/usr/bin/env python3
"""Check the app's text against docs/writing-style.md#app-text.

Reads the string literals in src/**/*.rs, outside test code, and reports text
that breaks a rule the script can check. Exits 1 when it finds any.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
STATUS_LIMIT = 60

STRING = re.compile(r'"(?:[^"\\\n]|\\.)*"')
RULES = [
    (
        re.compile(
            r"\b((?:stereo)?centres?|centred|symmetris\w*|colours?|cancell\w*|analys(?:e|ed|ing)|neighbours?)\b",
            re.IGNORECASE,
        ),
        "write US English",
    ),
    (re.compile(r"\w\(s\)"), "write a real plural, not (s)"),
    (re.compile(r"\bEscape\b"), "name the key Esc"),
    (
        re.compile(r"\bUndo (?:restores|removes|can remove)\b"),
        "state the result; Undo is always there",
    ),
]
SECTION = re.compile(r'\bsection\(\s*"((?:[^"\\]|\\.)*)"')
STATUS = re.compile(r'\bstatus\s*=\s*(?:format!\(\s*)?"((?:[^"\\]|\\.)*)"')
PLACEHOLDER = re.compile(r"\{[^{}]*\}")
TEST_MODULE = re.compile(r"#\[cfg\(test\)\]\s*mod \w+\s*\{")


def is_test_file(path: Path) -> bool:
    name = path.name
    return "tests" in path.parts or name == "tests.rs" or name.endswith(("_tests.rs", "_qa.rs"))


def app_code(text: str) -> str:
    """The file without its inline test module and comment lines, keeping
    line numbers."""
    test = TEST_MODULE.search(text)
    if test:
        text = text[: test.start()]
    return "\n".join("" if line.lstrip().startswith("//") else line for line in text.split("\n"))


def problems(path: Path, text: str):
    code = app_code(text)

    def line(offset: int) -> int:
        return code.count("\n", 0, offset) + 1

    for literal in STRING.finditer(code):
        for pattern, rule in RULES:
            found = pattern.search(literal.group())
            if found:
                yield line(literal.start()), f"{rule}: {found.group()!r} in {literal.group()}"
    for heading in SECTION.finditer(code):
        label = heading.group(1)
        if any(c.isalpha() for c in label) and label == label.upper():
            yield (
                line(heading.start()),
                f"write section headings in sentence case; section() adds capitals: {label!r}",
            )
    for status in STATUS.finditer(code):
        shown = PLACEHOLDER.sub("", status.group(1))
        if len(shown) > STATUS_LIMIT:
            yield (
                line(status.start()),
                f"status messages stay within {STATUS_LIMIT} characters "
                f"({len(shown)}): {status.group(1)!r}",
            )


def main() -> int:
    found = 0
    for path in sorted((ROOT / "src").rglob("*.rs")):
        if is_test_file(path):
            continue
        for number, message in problems(path, path.read_text(encoding="utf-8")):
            print(f"{path.relative_to(ROOT)}:{number}: {message}")
            found += 1
    if found:
        print(f"{found} app text problems; see docs/writing-style.md#app-text", file=sys.stderr)
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
