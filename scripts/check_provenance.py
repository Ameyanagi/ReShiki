"""Check the Phase A naming ledger without rewriting source files or recorded hashes.

Rust files carry opening SPDX/Origin headers; mixed files additionally mark the
functions listed in ``parts``. Other files are checked byte for byte with SHA-256.
The small Rust scanner only locates declarations and their leading comments: it
does not interpret function bodies or attempt to infer upstream ancestry.
"""

import argparse
import hashlib
import re
import sys
import tomllib
from dataclasses import dataclass
from pathlib import Path, PurePosixPath

LEDGER_NAME = "PROVENANCE-nazuchem.toml"
REQUIRED_ROOTS = ("crates/nazuchem-kernel", "crates/opsin", "src/naming/openclatura")
REQUIRED_FIXTURES = (
    "src/naming/catalog/data.json",
    "src/naming/catalog/provenance.json",
    "tests/fixtures/openclatura/graph_oracle.json",
    "tests/fixtures/s2n/bakeoff_corpus.tsv",
    "tests/fixtures/s2n/baseline.tsv",
    "tests/fixtures/s2n/metamorphic_known_failures.tsv",
    "tests/fixtures/s2n/name_known_failures.tsv",
    "tests/fixtures/s2n/openclatura_tests.tsv",
    "tests/fixtures/s2n/rules_cases.tsv",
    "tests/fixtures/s2n/status-snapshot.tsv",
    "tests/fixtures/s2n/worker_snapshot.jsonl",
)
IDENTIFIER = r"(?:r#)?[A-Za-z_][A-Za-z_0-9]*"
RUST_TOKEN = re.compile(rf"\bfn\s+({IDENTIFIER})|\bmod\s+({IDENTIFIER})|\bimpl\b|[{{}};\[\]]")
RAW_LITERAL = re.compile(r'(?:br|cr|r)(#*)"')
CHAR_LITERAL = re.compile(r"'(?:\\(?:[^\n]|u\{[^}]*\})|[^'\\\n])'")


@dataclass(frozen=True)
class RustFunction:
    name: str
    qualified_name: str
    line: int
    prelude: str


def mask_rust(text: str, comments: list[str] | None = None) -> str:
    """Blank comments and Rust literals, preserving positions and newlines."""
    result = list(text)
    index = 0
    while index < len(text):
        start = index
        if text.startswith("//", index):
            end = text.find("\n", index)
            index = len(text) if end < 0 else end
            if comments is not None:
                comments.append(text[start:index])
        elif text.startswith("/*", index):
            index += 2
            depth = 1
            while index < len(text) and depth:
                if text.startswith("/*", index):
                    depth += 1
                    index += 2
                elif text.startswith("*/", index):
                    depth -= 1
                    index += 2
                else:
                    index += 1
        elif raw := RAW_LITERAL.match(text, index):
            end = text.find('"' + raw[1], raw.end())
            index = len(text) if end < 0 else end + 1 + len(raw[1])
        elif text[index] == '"':
            index += 1
            while index < len(text):
                if text[index] == "\\":
                    index += 2
                elif text[index] == '"':
                    index += 1
                    break
                else:
                    index += 1
        elif char := CHAR_LITERAL.match(text, index):
            index = char.end()
        else:
            index += 1
            continue
        for offset in range(start, min(index, len(text))):
            if text[offset] != "\n":
                result[offset] = " "
    return "".join(result)


def impl_name(declaration: str) -> str:
    # Generic parameters and arguments can contain `for` and identifiers that
    # are not the implementing type. Keep only the outer declaration tokens.
    depth = 0
    outer = []
    for character in declaration:
        if character == "<":
            depth += 1
        elif character == ">":
            depth = max(0, depth - 1)
        elif not depth:
            outer.append(character)
    declaration = "".join(outer).split(" where ")[0]
    declaration = re.split(r"\bfor\b", declaration)[-1]
    names = re.findall(IDENTIFIER, declaration)
    return names[-1] if names else ""


def rust_functions(text: str) -> list[RustFunction]:
    masked = mask_rust(text)
    scopes: list[str] = []
    functions = []
    pending = ""
    impl_start = None
    square_depth = 0
    boundary = 0
    for token in RUST_TOKEN.finditer(masked):
        value = token[0]
        if value == "[":
            square_depth += 1
            continue
        if value == "]":
            square_depth -= 1
            continue
        if square_depth:
            continue
        if token[1]:
            name = token[1].removeprefix("r#")
            qualified = "::".join(scope for scope in [*scopes, name] if scope)
            functions.append(
                RustFunction(
                    name,
                    qualified,
                    text.count("\n", 0, token.start()) + 1,
                    text[boundary : token.start()],
                )
            )
            pending = name
            impl_start = None
        elif token[2]:
            pending = token[2].removeprefix("r#")
        elif value == "impl":
            # `impl Trait` inside a function's parameter list is not a scope.
            if not pending:
                impl_start = token.end()
        elif value == "{":
            if impl_start is not None:
                pending = impl_name(masked[impl_start : token.start()])
            scopes.append(pending)
            pending = ""
            impl_start = None
            boundary = token.end()
        elif value == "}":
            if scopes:
                scopes.pop()
            pending = ""
            impl_start = None
            boundary = token.end()
        elif value == ";":
            pending = ""
            impl_start = None
            boundary = token.end()
    return functions


def correct_marker(prelude: str, part: dict) -> bool:
    comments: list[str] = []
    mask_rust(prelude, comments)
    prelude = "\n".join(comments)
    origin = part.get("origin")
    if origin == "original":
        return bool(re.search(r"^\s*// Original nazuchem code \(not in .+\)", prelude, re.M))
    verb = {"port": "Ported", "copy": "Ported", "adapted": "Adapted", "mixed": "Adapted"}.get(
        origin
    )
    if verb is None:
        return False
    for source in part.get("sources", []):
        path = source.get("path")
        if isinstance(path, str):
            for marker in re.finditer(rf"^\s*// {verb} from (.+)$", prelude, re.M):
                if re.search(rf"(?:^|,\s*){re.escape(path)}(?=::|[,\s]|$)", marker[1]):
                    return True
    return False


def check_rust(path: str, text: str, entry: dict) -> list[str]:
    errors = []
    code = mask_rust(text)
    first_code = re.search(r"\S", code)
    header = text[: first_code.start()] if first_code else text
    header_comments: list[str] = []
    mask_rust(header, header_comments)
    header = "\n".join(header_comments)
    identifiers = re.findall(r"^[ \t]*// SPDX-License-Identifier:[ \t]*(.*?)[ \t]*$", header, re.M)
    if identifiers != [entry.get("license")]:
        errors.append(
            f"{path}: opening SPDX header must match ledger license {entry.get('license')!r}"
        )
    if not re.search(r"^[ \t]*// Origin:[ \t]*\S.*$", header, re.M):
        errors.append(f"{path}: missing opening // Origin: header")
    if entry.get("origin") != "mixed":
        return errors
    functions = rust_functions(text)
    for part in entry.get("parts", []):
        for item in part.get("items", []):
            if not isinstance(item, str) or not item.startswith("fn "):
                errors.append(f"{path}: unsupported mixed-file item {item!r}; expected 'fn name'")
                continue
            name = item.removeprefix("fn ")
            matches = [
                function
                for function in functions
                if function.qualified_name == name
                or function.qualified_name.endswith("::" + name)
                or ("::" not in name and function.name == name)
            ]
            if not matches:
                errors.append(f"{path}: ledger item {item!r} has no Rust function")
            for function in matches:
                if not correct_marker(function.prelude, part):
                    errors.append(
                        f"{path}:{function.line}: {item} missing {part.get('origin')!r} "
                        "provenance marker matching ledger sources"
                    )
    return errors


def check_provenance(root: Path, ledger_path: Path | None = None) -> list[str]:
    root = root.resolve()
    ledger_path = ledger_path or root / LEDGER_NAME
    try:
        with ledger_path.open("rb") as ledger_file:
            ledger = tomllib.load(ledger_file)
    except (OSError, tomllib.TOMLDecodeError) as error:
        return [f"{ledger_path}: cannot read provenance ledger: {error}"]
    entries = ledger.get("file")
    if ledger.get("schema_version", 1) != 1 or not isinstance(entries, list):
        return [f"{ledger_path}: expected [[file]] entries and schema_version 1 if specified"]
    errors = []
    recorded = {}
    for entry in entries:
        if not isinstance(entry, dict) or not isinstance(entry.get("path"), str):
            errors.append(f"{ledger_path}: every [[file]] entry requires a string path")
            continue
        path = entry["path"]
        relative = PurePosixPath(path)
        if relative.is_absolute() or ".." in relative.parts or relative.as_posix() != path:
            errors.append(f"{ledger_path}: invalid repository-relative path {path!r}")
            continue
        if path in recorded:
            errors.append(f"{path}: duplicate ledger entry")
            continue
        recorded[path] = entry
        target = root / path
        if not target.is_file():
            errors.append(f"{path}: ledger path does not exist as a file")
            continue
        if not target.resolve().is_relative_to(root):
            errors.append(f"{path}: ledger path resolves outside the repository")
            continue
        try:
            contents = target.read_bytes()
            if target.suffix == ".rs":
                errors.extend(check_rust(path, contents.decode("utf-8"), entry))
            else:
                digest = hashlib.sha256(contents).hexdigest()
                if entry.get("sha256") != digest:
                    errors.append(f"{path}: SHA-256 mismatch; actual {digest}")
        except (OSError, UnicodeError) as error:
            errors.append(f"{path}: cannot read file: {error}")
    required = set(REQUIRED_FIXTURES)
    for prefix in REQUIRED_ROOTS:
        directory = root / prefix
        if not directory.is_dir():
            errors.append(f"{prefix}: required provenance directory does not exist")
            continue
        required.update(
            path.relative_to(root).as_posix() for path in directory.rglob("*") if path.is_file()
        )
    errors.extend(f"{path}: missing ledger entry" for path in sorted(required - recorded.keys()))
    return errors


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument("--ledger", type=Path, help=f"Ledger path (default: <root>/{LEDGER_NAME})")
    args = parser.parse_args(argv)
    errors = check_provenance(args.root, args.ledger)
    if errors:
        print("Provenance check failed:", file=sys.stderr)
        for error in errors:
            print(f"  {error}", file=sys.stderr)
        return 1
    print("Naming provenance headers, function markers and file hashes match the ledger.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
