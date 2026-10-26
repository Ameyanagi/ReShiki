"""Guard the reviewed 2026-10-26 dependency fixes and unreachable decoder paths.

This is a dated regression guard, not a current advisory database scan. See
docs/changes/security-dependency-audit.md for the exact VEX dispositions.
"""

import ast
import json
import re
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CARGO_MINIMUMS = {
    "rustls": (0, 23, 46),
    "rustls-webpki": (0, 103, 16),
    "tokio": (1, 53, 3),
    "webpki-root-certs": (1, 0, 10),
}
NPM_MINIMUMS = {
    "fast-uri": (3, 1, 9),
    "postcss-selector-parser": (7, 1, 6),
    "sharp": (0, 35, 5),
    "smol-toml": (1, 9, 0),
    "source-map-js": (1, 2, 2),
    "undici": (8, 12, 2),
    "vite": (8, 3, 3),
    "zod": (4, 6, 6),
}
REVIEWED_CARGO = {
    "lru": ("0.16.4", "7f66e8d5d03f609abc3a39e6f08e4164ebf1447a732906d39eb9b99b7919ef39"),
    "cryoglyph": ("0.1.0", "08bc795bdbccdbd461736fb163930a009da6597b226d6f6fce33e7a8eb6ec519"),
    "cosmic-text": ("0.15.0", "173852283a9a57a3cbe365d86e74dc428a09c50421477d5ad6fe9d9509e37737"),
}
ASTRO_INTEGRITY = (
    "sha512-NF08hk3edFkVmr9avf9HW/rQyf6NrpbVDyYj2cVN3JfijDhfJYh1ivWVTY5PUd5+"
    "rRZn/Vtu4iaeyNlP0Iv6mg=="
)
FONT_MODULES = (
    "PIL.ImageFont",
    "PIL.ImageDraw",
    "PIL._imagingft",
    "rdkit.Chem.Draw",
    "rdkit.Chem.rdMolDraw2D",
)


def bun_json(text):
    """Remove trailing commas outside quoted strings, preserving lock values."""
    result = []
    quoted = escape = False
    for index, char in enumerate(text):
        if quoted:
            result.append(char)
            if escape:
                escape = False
            elif char == "\\":
                escape = True
            elif char == '"':
                quoted = False
        else:
            if char == '"':
                quoted = True
            if char == ",":
                following = index + 1
                while following < len(text) and text[following].isspace():
                    following += 1
                if following < len(text) and text[following] in "}]":
                    continue
            result.append(char)
    return json.loads("".join(result))


def version(text):
    if not re.fullmatch(r"\d+\.\d+\.\d+", text):
        raise ValueError(f"Review non-release version {text!r}")
    return tuple(map(int, text.split(".")))


def font_api_violations(source):
    """Fail closed when pixel-only Python code starts using font decoders."""
    tree = ast.parse(source)
    aliases = {}
    failures = set()

    def forbidden(name):
        return any(name == prefix or name.startswith(prefix + ".") for prefix in FONT_MODULES)

    def qualified(node):
        if isinstance(node, ast.Name):
            return aliases.get(node.id, node.id)
        if isinstance(node, ast.Attribute):
            return qualified(node.value) + "." + node.attr
        return ""

    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            for item in node.names:
                aliases[item.asname or item.name.split(".")[0]] = (
                    item.name if item.asname else item.name.split(".")[0]
                )
                if forbidden(item.name):
                    failures.add((node.lineno, item.name))
        elif isinstance(node, ast.ImportFrom):
            for item in node.names:
                name = f"{node.module}.{item.name}"
                aliases[item.asname or item.name] = name
                if forbidden(name) or (item.name == "*" and node.module in {"PIL", "rdkit.Chem"}):
                    failures.add((node.lineno, name))
    for node in ast.walk(tree):
        name = qualified(node)
        if isinstance(node, (ast.Name, ast.Attribute)) and forbidden(name):
            failures.add((node.lineno, name))
        if isinstance(node, ast.Call) and node.args:
            name = qualified(node.func)
            if name in {"__import__", "importlib.import_module"} and isinstance(
                node.args[0], ast.Constant
            ):
                target = node.args[0].value
                if isinstance(target, str) and forbidden(target):
                    failures.add((node.lineno, target))
            if name == "getattr" and len(node.args) > 1 and isinstance(node.args[1], ast.Constant):
                target = qualified(node.args[0]) + "." + str(node.args[1].value)
                if forbidden(target):
                    failures.add((node.lineno, target))
    return sorted(failures)


def lru_scope_failures(cargo):
    failures = []
    old_lru = [p for p in cargo if p["name"] == "lru" and version(p["version"]) < (0, 18, 2)]
    if old_lru:
        parents = [
            p["name"]
            for p in cargo
            if any(d.split()[0] == "lru" for d in p.get("dependencies", []))
        ]
        if parents != ["cryoglyph"]:
            failures.append(
                "Reassess lru: a consumer can introduce a panicking-Drop key/pop() path"
            )
        for name, (reviewed_version, checksum) in REVIEWED_CARGO.items():
            found = [p for p in cargo if p["name"] == name]
            if len(found) != 1 or (found[0]["version"], found[0].get("checksum")) != (
                reviewed_version,
                checksum,
            ):
                failures.append(f"Reassess lru's reviewed {name} source/Copy-key invariant")
    return failures


def http_cache_scope_failures(bun, config):
    failures = []
    http_cache = []
    for key, entry in bun.items():
        name, resolved = entry[0].rsplit("@", 1)
        if name == "http-cache-semantics":
            http_cache.append((key, resolved))
    if http_cache:
        parents = [
            key
            for key, entry in bun.items()
            if len(entry) > 2 and "http-cache-semantics" in entry[2].get("dependencies", {})
        ]
        astro = bun.get("astro", [])
        if http_cache != [("http-cache-semantics", "4.2.0")] or parents != ["astro"]:
            failures.append(
                "Reassess http-cache-semantics consumers/version and max-stale reachability"
            )
        if not astro or astro[0] != "astro@7.3.3" or astro[-1] != ASTRO_INTEGRITY:
            failures.append("Reassess Astro image-build TTL-only cache usage")
        if not re.search(r'^\s*output:\s*["\']static["\'],?\s*$', config, re.MULTILINE):
            failures.append("The reviewed HTTP cache disposition requires static website output")
        if re.search(r"\badapter\s*:", config):
            failures.append("Reassess HTTP cache reachability for a website server adapter")
    return failures


def direct_http_cache_import(source):
    return bool(
        re.search(
            r"""(?:\bfrom\s*|\b(?:import|require)\s*\(\s*)["']http-cache-semantics["']""",
            source,
        )
    )


def check(root=ROOT):
    cargo = tomllib.loads((root / "Cargo.lock").read_text())["package"]
    failures = lru_scope_failures(cargo)
    for package in cargo:
        minimum = CARGO_MINIMUMS.get(package["name"])
        if minimum and version(package["version"]) < minimum:
            failures.append(f"{package['name']} is below the reviewed security minimum")
    bun = bun_json((root / "bun.lock").read_text())["packages"]
    failures.extend(http_cache_scope_failures(bun, (root / "website/astro.config.mjs").read_text()))
    for key, entry in bun.items():
        name, resolved = entry[0].rsplit("@", 1)
        if name in NPM_MINIMUMS and version(resolved) < NPM_MINIMUMS[name]:
            failures.append(f"{key} is below the reviewed security minimum")
    python = tomllib.loads((root / "uv.lock").read_text())["package"]
    if not any(package["name"] == "certifi" for package in python):
        failures.append("The CI TLS helper requires the reviewed certifi CA bundle")
    for package in python:
        if package["name"] == "pillow" and version(package["version"]) < (13, 0, 0):
            failures.append("Pillow is below the reviewed pixel-codec security minimum")
        if package["name"] == "certifi" and version(package["version"]) < (2026, 10, 19):
            failures.append("certifi retains the distrusted emSign root")
    for directory in ("scripts", "reference", "tests"):
        for path in sorted((root / directory).rglob("*.py")):
            for line, name in font_api_violations(path.read_text(encoding="utf-8")):
                failures.append(
                    f"{path.relative_to(root)}:{line}: reassess native font decoding through {name}"
                )
    for directory in (
        "scripts",
        "integrations",
        "packages/editor",
        "website/src",
        "website/scripts",
    ):
        for path in sorted((root / directory).rglob("*")):
            if path.suffix not in {".js", ".mjs", ".ts", ".astro"} or any(
                part in {"node_modules", "dist"} for part in path.relative_to(root).parts
            ):
                continue
            if direct_http_cache_import(path.read_text(encoding="utf-8")):
                failures.append(
                    f"{path.relative_to(root)}: reassess direct shared-cache policy use"
                )
    return failures


if __name__ == "__main__":
    errors = check()
    if errors:
        raise SystemExit("\n".join(errors))
    print("Dated dependency fixes and scoped pixel/cache/Copy-key dispositions verified.")
