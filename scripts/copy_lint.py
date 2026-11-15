#!/usr/bin/env python3
"""Check the app's text against docs/writing-style.md#app-text.

Reads the string literals in src/**/*.rs, outside test code, and reports text
that breaks a rule the script can check. Exits 1 when it finds any.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
STATUS_LIMIT = 60
# A {placeholder} counts as a short word, such as "Light" or "12.5".
PLACEHOLDER_WIDTH = 5

# String literals, including raw and multiline ones; char literals are matched
# so that '"' does not open a string.
TOKEN = re.compile(
    r"""(?<![\w])b?r(?P<hashes>\#*)"(?P<raw>.*?)"(?P=hashes)"""
    r"""|b?"(?P<text>(?:[^"\\]|\\.)*)\""""
    r"""|'(?:[^'\\\n]|\\[^\n]+?)'""",
    re.DOTALL,
)
RULES = [
    (
        re.compile(
            r"\b((?:stereo)?centres?|centred|symmetris\w*|colours?|cancell(?:ed|ing)|analys(?:e|ed|ing)|neighbours?)\b",
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
STATUS = re.compile(r"\bstatus\s*=(?![=>])")
CONSTANT = re.compile(r"\bconst\s+([A-Z][A-Z0-9_]*)\s*:\s*&(?:'static\s+)?str\s*=\s*")
NAME = re.compile(r"\b[A-Z][A-Z0-9_]+\b")
PLACEHOLDER = re.compile(r"\{[^{}]*\}")
CONTINUATION = re.compile(r"\\\n\s*")
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


def literals(code: str, start: int = 0, end: int | None = None):
    """(offset, text) of each string literal, with line continuations joined
    and \\n and \\t read as the characters they stand for."""
    for token in TOKEN.finditer(code, start, len(code) if end is None else end):
        if token.group("raw") is not None:
            yield token.start(), token.group("raw")
        elif token.group("text") is not None:
            text = CONTINUATION.sub("", token.group("text"))
            yield token.start(), text.replace("\\n", "\n").replace("\\t", "\t")


def constants(code: str) -> dict[str, str]:
    found = {}
    for constant in CONSTANT.finditer(code):
        literal = next(literals(code, constant.end()), None)
        if literal and literal[0] == constant.end():
            found[constant.group(1)] = literal[1]
    return found


def expression_end(code: str, start: int) -> int:
    """The `;` that ends the expression starting at `start`."""
    depth = 0
    position = start
    while position < len(code):
        token = TOKEN.match(code, position)
        if token:
            position = token.end()
            continue
        char = code[position]
        if char in "([{":
            depth += 1
        elif char in ")]}":
            depth -= 1
            if depth < 0:
                return position
        elif char == ";" and depth == 0:
            return position
        position += 1
    return position


def shown_length(message: str) -> int:
    first_line = message.split("\n", 1)[0]
    return len(PLACEHOLDER.sub("", first_line)) + PLACEHOLDER_WIDTH * len(
        PLACEHOLDER.findall(first_line)
    )


def problems(path: Path, text: str, shared: dict[str, str] | None = None):
    """Rule breaks in one file; `shared` holds string constants from other files."""
    code = app_code(text)
    named = {**(shared or {}), **constants(code)}

    def line(offset: int) -> int:
        return code.count("\n", 0, offset) + 1

    for offset, literal in literals(code):
        for pattern, rule in RULES:
            found = pattern.search(literal)
            if found:
                yield line(offset), f"{rule}: {found.group()!r} in {literal!r}"
    for heading in SECTION.finditer(code):
        label = heading.group(1)
        if any(c.isalpha() for c in label) and label == label.upper():
            yield (
                line(heading.start()),
                f"write section headings in sentence case; section() adds capitals: {label!r}",
            )
    for status in STATUS.finditer(code):
        end = expression_end(code, status.end())
        messages = [literal for _, literal in literals(code, status.end(), end)]
        messages += [
            named[name.group()]
            for name in NAME.finditer(code, status.end(), end)
            if name.group() in named
        ]
        for message in messages:
            length = shown_length(message)
            if length > STATUS_LIMIT:
                yield (
                    line(status.start()),
                    f"status messages stay within {STATUS_LIMIT} characters ({length}): {message!r}",
                )


def main() -> int:
    files = [path for path in sorted((ROOT / "src").rglob("*.rs")) if not is_test_file(path)]
    texts = {path: path.read_text(encoding="utf-8") for path in files}
    shared = {}
    for text in texts.values():
        shared.update(constants(app_code(text)))
    found = 0
    for path, text in texts.items():
        for number, message in problems(path, text, shared):
            print(f"{path.relative_to(ROOT)}:{number}: {message}")
            found += 1
    if found:
        print(f"{found} app text problems; see docs/writing-style.md#app-text", file=sys.stderr)
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
