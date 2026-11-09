"""Check the Phase A naming ledger without rewriting source files or recorded hashes.

Rust files carry exact opening SPDX/Origin/Source headers. Every function in a
mixed file belongs to an explicitly marked function or enclosing module/impl
region; a more specific assignment overrides the region. Each region records all
of its function descendants so new functions require an explicit ledger decision.
Other files are checked byte for byte with SHA-256. The scanner locates declarations
and their comments; it does not interpret bodies or attempt to infer upstream ancestry.
"""

import argparse
import hashlib
import json
import re
import sys
import tomllib
import unicodedata
from collections import Counter
from dataclasses import dataclass, replace
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
# Rust identifiers use Unicode XID_Start/XID_Continue. Match a superset so newer
# Unicode characters cannot disappear from the inventory; validate with Python's
# XID-compatible identifier rules below and fail closed if unsupported. Rust's
# Pattern_White_Space additionally includes the left/right directional marks.
RUST_SPACE = r"[\s\u200e\u200f]"
IDENTIFIER_START = r"(?:[^\W\d]|[^\x00-\x7f\s\u200e\u200f])"
IDENTIFIER_CONTINUE = r"(?:\w|[^\x00-\x7f\s\u200e\u200f])"
IDENTIFIER = rf"(?:r#)?{IDENTIFIER_START}{IDENTIFIER_CONTINUE}*"
KEYWORD_PREFIX = rf"(?<!{IDENTIFIER_CONTINUE})(?<!#)"
RUST_TOKEN = re.compile(
    rf"{KEYWORD_PREFIX}(fn|mod|trait){RUST_SPACE}+({IDENTIFIER})"
    rf"|{KEYWORD_PREFIX}impl(?!{IDENTIFIER_CONTINUE})|[{{}};\[\]()<>]"
)
RAW_LITERAL = re.compile(r'(?:br|cr|r)(#*)"')
CHAR_LITERAL = re.compile(r"'(?:\\(?:[^\n]|u\{[^}]*\})|[^'\\\n])'")
ORIGINS = {"original", "port", "copy", "adapted", "mixed", "data"}
MODULE_SCOPE_SYMBOL = "module scope (mapping uncertain)"
# These owner-deferred files are the only legacy-format exceptions. Pin both
# source bytes and canonical ledger contents so the exception cannot spread or
# silently accommodate changes to unresolved provenance.
FROZEN_PENDING = {
    "crates/nazuchem-kernel/src/native_order/linux_sort.rs": (
        "cf814c15f7111d5473a0b8add9683c1a9ca49229eb4ed62e28c210ae099e43c0",
        "f1bebe4cd078021f1ae6408364931aaa31e41aa251ba88b8b23d850770cf2611",
    ),
    "crates/nazuchem-kernel/src/native_order/sort_common.rs": (
        "98660b6f23fb85316695b08855f646523208ef0bd625603e951c3c89e0fbf909",
        "77f13e9de2d20a465ebb4b6c041babccfaa38d44d6b002f3522c167556c36146",
    ),
}


@dataclass(frozen=True)
class RustFunction:
    name: str
    qualified_name: str
    line: int
    prelude: str


@dataclass(frozen=True)
class RustItem:
    kind: str
    name: str
    qualified_name: str
    line: int
    prelude: str
    start: int
    body_start: int | None
    end: int
    parent: int | None
    ancestors: tuple[int, ...]

    @property
    def identity(self) -> str:
        return f"{self.kind} {self.qualified_name}"


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


def impl_name(declaration: str, *, preserve_generics: bool = False) -> str:
    """Name inherent impls by type and trait impls by fully qualified identity."""
    declaration = re.sub(RUST_SPACE, " ", declaration).strip()
    # Leading impl generic parameters do not identify the implemented trait.
    if declaration.startswith("<"):
        depth = 0
        for index, character in enumerate(declaration):
            depth += (character == "<") - (character == ">")
            if not depth:
                declaration = declaration[index + 1 :].strip()
                break
    depth = 0
    split_for = None
    for index, character in enumerate(declaration):
        if character == "<":
            depth += 1
        elif character == ">":
            depth = max(0, depth - 1)
        elif not depth and re.match(r"\bwhere\b", declaration[index:]):
            declaration = declaration[:index].strip()
            break
        elif not depth and re.match(r"\bfor\b", declaration[index:]):
            split_for = index

    def compact(value: str) -> str:
        value = unicodedata.normalize("NFC", value)
        return re.sub(r"\s*([<>,:&])\s*", r"\1", " ".join(value.split()))

    if split_for is not None:
        trait = declaration[:split_for].strip()
        target = declaration[split_for + 3 :].strip()
        return f"<{compact(target)} as {compact(trait)}>"
    if preserve_generics:
        return compact(declaration)
    # Inherent generic impls share their type's namespace (Type::method).
    outer = re.sub(r"<.*>", "", declaration)
    return compact(outer).replace("r#", "")


def rust_items(text: str) -> list[RustItem]:
    """Locate fn/mod/impl declarations and their lexical scope relationships.

    Offsets are character offsets into ``text``. ``parent``/``ancestors`` index
    this source-ordered result. Traits contribute a namespace but are not region
    assignments; trait methods can be individually assigned as ``fn Trait::f``.
    Unsupported identifiers or declarations nested in a pending signature raise
    ValueError, so callers cannot accept an incomplete function inventory.
    """
    masked = mask_rust(text)
    scopes: list[tuple[str, int | None]] = []
    items: list[RustItem] = []
    impl_targets: dict[str, str] = {}
    pending: tuple[str, int | None] | None = None
    impl_start: int | None = None
    attribute_depth = 0
    square_depth = 0
    paren_depth = 0
    pending_parens = 0
    pending_squares = 0
    angle_depth = 0
    boundary = 0
    for token in RUST_TOKEN.finditer(masked):
        value = token[0]
        if value == "[":
            prefix = masked[: token.start()].rstrip()
            if attribute_depth or prefix.endswith(("#", "#!")):
                attribute_depth += 1
            else:
                square_depth += 1
            continue
        if value == "]":
            if attribute_depth:
                attribute_depth -= 1
            else:
                square_depth = max(0, square_depth - 1)
            continue
        if attribute_depth:
            continue
        if value == "(":
            paren_depth += 1
            continue
        if value == ")":
            paren_depth = max(0, paren_depth - 1)
            continue
        if value in {"<", ">"}:
            if pending and paren_depth == pending_parens and square_depth == pending_squares:
                angle_depth = max(0, angle_depth + (1 if value == "<" else -1))
            continue
        if token[1] or value == "impl":
            # `impl Trait` inside a function's parameter list is not a scope.
            if pending:
                if token[1]:
                    line = text.count("\n", 0, token.start()) + 1
                    raise ValueError(
                        f"nested {token[1]} declaration at line {line} inside pending "
                        f"declaration {pending[0]!r}; unsupported signature expression"
                    )
                continue
            if value == "impl" and "=" in masked[boundary : token.start()]:
                # `type Alias = impl Trait` is a type expression, not an impl.
                continue
            kind = token[1] or "impl"
            name = token[2].removeprefix("r#") if token[2] else ""
            if name and not name.isidentifier():
                line = text.count("\n", 0, token.start()) + 1
                raise ValueError(f"unsupported Rust identifier {name!r} at line {line}")
            name = unicodedata.normalize("NFC", name)
            ancestors = tuple(index for _, index in scopes if index is not None)
            index = None
            if kind != "trait":
                index = len(items)
                qualified = "::".join(scope for scope, _ in scopes if scope)
                qualified = "::".join(filter(None, (qualified, name)))
                items.append(
                    RustItem(
                        kind,
                        name,
                        qualified,
                        text.count("\n", 0, token.start()) + 1,
                        text[boundary : token.start()],
                        token.start(),
                        None,
                        len(text),
                        ancestors[-1] if ancestors else None,
                        ancestors,
                    )
                )
            pending = (name, index)
            pending_parens = paren_depth
            pending_squares = square_depth
            angle_depth = 0
            if kind == "impl":
                impl_start = token.end()
        elif value == "{":
            if (
                pending
                and paren_depth == pending_parens
                and square_depth == pending_squares
                and not angle_depth
            ):
                name, index = pending
                if impl_start is not None:
                    name = impl_name(masked[impl_start : token.start()])
                if index is not None:
                    qualified = "::".join(scope for scope, _ in scopes if scope)
                    qualified = "::".join(filter(None, (qualified, name)))
                    if impl_start is not None:
                        target = impl_name(
                            masked[impl_start : token.start()], preserve_generics=True
                        )
                        previous = impl_targets.setdefault(qualified, target)
                        if previous != target:
                            raise ValueError(
                                f"ambiguous impl identity {qualified!r} for distinct targets "
                                f"{previous!r} and {target!r}; concrete generic impls need "
                                "distinct scanner identities"
                            )
                    items[index] = replace(
                        items[index], name=name, qualified_name=qualified, body_start=token.start()
                    )
                scopes.append((name, index))
                pending = None
                impl_start = None
            else:
                scopes.append(("", None))
            boundary = token.end()
        elif value == "}":
            if scopes:
                _, index = scopes.pop()
                if index is not None:
                    items[index] = replace(items[index], end=token.end())
            boundary = token.end()
        elif value == ";":
            if pending and pending[1] is not None:
                items[pending[1]] = replace(items[pending[1]], end=token.end())
            pending = None
            impl_start = None
            angle_depth = 0
            boundary = token.end()
    return items


def rust_functions(text: str) -> list[RustFunction]:
    """Retain the public function-scanner API for callers and migration tools."""
    return [
        RustFunction(item.name, item.qualified_name, item.line, item.prelude)
        for item in rust_items(text)
        if item.kind == "fn"
    ]


def comment_fields(text: str, field: str) -> list[str]:
    comments: list[str] = []
    mask_rust(text, comments)
    return [
        match[1]
        for comment in comments
        if (match := re.fullmatch(rf"// {re.escape(field)}:[ \t]*(.*?)[ \t]*", comment))
    ]


def source_identities(
    sources: object, fields: tuple[str, ...], *, unique: bool = True
) -> list[tuple[str, ...]] | None:
    if not isinstance(sources, list) or not sources:
        return None
    identities = []
    for source in sources:
        if not isinstance(source, dict):
            return None
        values = tuple(source.get(field) for field in fields)
        if any(not isinstance(value, str) or not value.strip() for value in values):
            return None
        identities.append(values)
    if unique and len(set(identities)) != len(identities):
        return None
    return identities


def correct_marker(
    prelude: str, part: dict, identity: str, inherited_sources: set[tuple[str, ...]]
) -> bool:
    """Match every specific source; module mappings may inherit validated headers."""
    markers = comment_fields(prelude, "Provenance")
    if markers != [f"{identity} | {part.get('origin')}"]:
        return False
    sources = source_identities(
        part.get("sources"), ("project", "path", "ref", "symbol"), unique=False
    )
    if sources is None:
        return False
    # File Source headers before the marker are a separate namespace.
    comments: list[str] = []
    mask_rust(prelude, comments)
    marker_index = next(
        index for index, comment in enumerate(comments) if comment.startswith("// Provenance:")
    )
    marker_sources = comment_fields("\n".join(comments[marker_index + 1 :]), "Source")
    expected = {f"{project} | {path}::{symbol}" for project, path, _, symbol in sources}
    required = {
        f"{project} | {path}::{symbol}"
        for project, path, ref, symbol in sources
        if symbol != MODULE_SCOPE_SYMBOL or (project, path, ref) not in inherited_sources
    }
    actual = Counter(marker_sources)
    return (
        all(count == 1 for count in actual.values())
        and required.issubset(actual)
        and actual.keys() <= expected
    )


def check_region_members(
    path: str, part: dict, by_identity: dict[str, list[RustItem]], items: list[RustItem]
) -> list[str]:
    """Require an exact inventory for each lexical mod/impl assignment in a part."""
    regions = {
        identity
        for identity in part["items"]
        if isinstance(identity, str) and identity.startswith(("mod ", "impl "))
    }
    members = part.get("members", {})
    if not isinstance(members, dict):
        return [f"{path}: part members must be a table keyed by exact mod/impl identities"]
    errors = [
        f"{path}: members inventory key {identity!r} has no mod/impl item in this part"
        for identity in members.keys() - regions
    ]
    for identity in sorted(regions):
        inventory = members.get(identity)
        if not isinstance(inventory, list) or any(
            not isinstance(member, str) or not re.fullmatch(r"fn \S(?:.*\S)?", member)
            for member in inventory
        ):
            errors.append(
                f"{path}: region {identity!r} requires a members inventory of exact fn identities"
            )
            continue
        recorded = Counter(inventory)
        if any(count > 1 for count in recorded.values()):
            errors.append(f"{path}: region {identity!r} members inventory contains duplicates")
        matches = by_identity.get(identity, [])
        if len(matches) > 1:
            errors.append(f"{path}: region {identity!r} has an ambiguous Rust declaration identity")
        for region in matches:
            actual = Counter(
                item.identity
                for item in items
                if item.kind == "fn" and any(items[index] is region for index in item.ancestors)
            )
            missing = sorted((actual - recorded).elements())
            stale = sorted((recorded - actual).elements())
            if missing:
                errors.append(
                    f"{path}:{region.line}: region {identity!r} members inventory missing "
                    + ", ".join(repr(member) for member in missing)
                )
            if stale:
                errors.append(
                    f"{path}:{region.line}: region {identity!r} members inventory has stale "
                    + ", ".join(repr(member) for member in stale)
                )
    return errors


def ledger_entry_digest(entry: dict) -> str:
    canonical = json.dumps(entry, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
    return hashlib.sha256(canonical.encode("utf-8")).hexdigest()


def check_rust(path: str, text: str, entry: dict) -> list[str]:
    if frozen := FROZEN_PENDING.get(path):
        errors = []
        if hashlib.sha256(text.encode("utf-8")).hexdigest() != frozen[0]:
            errors.append(f"{path}: frozen pending provenance source snapshot changed")
        try:
            digest = ledger_entry_digest(entry)
        except (TypeError, ValueError):
            digest = None
        if digest != frozen[1]:
            errors.append(f"{path}: frozen pending provenance ledger entry changed")
        return errors
    errors = []
    code = mask_rust(text)
    first_code = re.search(r"\S", code)
    header = text[: first_code.start()] if first_code else text
    header_comments: list[str] = []
    mask_rust(header, header_comments)
    # A first declaration's own marker may precede the first code token. Its
    # Source symbols are not file-header Source triples.
    marker = next(
        (
            index
            for index, comment in enumerate(header_comments)
            if comment.startswith("// Provenance:")
        ),
        len(header_comments),
    )
    header = "\n".join(header_comments[:marker])
    identifiers = comment_fields(header, "SPDX-License-Identifier")
    if identifiers != [entry.get("license")]:
        errors.append(
            f"{path}: opening SPDX header must match ledger license {entry.get('license')!r}"
        )
    if not isinstance(entry.get("origin"), str) or entry["origin"] not in ORIGINS:
        errors.append(f"{path}: unsupported ledger origin {entry.get('origin')!r}")
    if comment_fields(header, "Origin") != [entry.get("origin")]:
        errors.append(
            f"{path}: opening // Origin: header must match ledger origin {entry.get('origin')!r}"
        )
    sources = source_identities(entry.get("sources"), ("project", "path", "ref"))
    if sources is None:
        errors.append(f"{path}: file sources require unique nonempty project, path and ref fields")
    else:
        expected = [" | ".join(source) for source in sources]
        if Counter(comment_fields(header, "Source")) != Counter(expected):
            errors.append(
                f"{path}: opening // Source: headers must match every ledger project, path and ref"
            )
    if entry.get("origin") != "mixed":
        if comment_fields(text, "Provenance"):
            errors.append(f"{path}: // Provenance: markers require mixed-file origin and parts")
        return errors
    inherited_sources = set(sources) if sources is not None and not errors else set()

    try:
        items = rust_items(text)
    except ValueError as error:
        errors.append(f"{path}: cannot verify complete Rust inventory: {error}")
        return errors
    by_identity: dict[str, list[RustItem]] = {}
    for item in items:
        by_identity.setdefault(item.identity, []).append(item)
    assigned: dict[str, dict] = {}
    parts = entry.get("parts")
    if not isinstance(parts, list) or not parts:
        errors.append(f"{path}: mixed file requires nonempty parts")
        parts = []
    for part in parts:
        if not isinstance(part, dict):
            errors.append(f"{path}: mixed-file part must be a table")
            continue
        if not isinstance(part.get("origin"), str) or part["origin"] not in ORIGINS - {"data"}:
            errors.append(f"{path}: unsupported mixed-file part origin {part.get('origin')!r}")
        part_sources = source_identities(part.get("sources"), ("project", "path", "ref", "symbol"))
        part_file_sources = source_identities(
            part.get("sources"), ("project", "path", "ref"), unique=False
        )
        if part_sources is None or part_file_sources is None:
            errors.append(
                f"{path}: part sources require unique nonempty project, path, ref and symbol fields"
            )
        elif sources is not None and not set(part_file_sources).issubset(sources):
            errors.append(f"{path}: part sources must also occur in file sources")
        part_items = part.get("items")
        if not isinstance(part_items, list) or not part_items:
            errors.append(f"{path}: mixed-file part requires nonempty items")
            continue
        errors.extend(check_region_members(path, part, by_identity, items))
        for identity in part_items:
            if not isinstance(identity, str) or not re.fullmatch(
                r"(?:fn|mod|impl) \S(?:.*\S)?", identity
            ):
                errors.append(
                    f"{path}: unsupported mixed-file item {identity!r}; expected 'fn/mod/impl qualified_name'"
                )
                continue
            if identity in assigned:
                errors.append(f"{path}: duplicate mixed-file ledger item {identity!r}")
                continue
            assigned[identity] = part
            matches = by_identity.get(identity, [])
            if not matches:
                errors.append(
                    f"{path}: ledger item {identity!r} has no Rust declaration with that exact identity"
                )
            for item in matches:
                if not correct_marker(item.prelude, part, identity, inherited_sources):
                    errors.append(
                        f"{path}:{item.line}: {identity} missing {part.get('origin')!r} "
                        "provenance marker matching exact local identity and required ledger source symbols"
                    )
    for item in items:
        if item.identity in assigned:
            continue
        if comment_fields(item.prelude, "Provenance"):
            errors.append(
                f"{path}:{item.line}: {item.identity} provenance marker has no exact ledger item"
            )
        if item.kind != "fn":
            continue
        regions = [
            items[index]
            for index in reversed(item.ancestors)
            if items[index].kind in {"mod", "impl"} and items[index].identity in assigned
        ]
        if not regions:
            errors.append(
                f"{path}:{item.line}: {item.identity} has no mixed-file provenance assignment"
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
