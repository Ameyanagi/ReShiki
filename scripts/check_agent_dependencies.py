"""Audit the agent API's dependencies for license, network and serde_json drift.

For every release target, on normal and build edges only (dev-dependencies never
ship, so the interop test's rmcp `client` feature is out of scope):
- serde_json features stay within the baseline and never include preserve_order
  or arbitrary_precision, which would change the byte-pinned Codex tool JSON;
- rmcp features stay within the reviewed server-only allowlist;
- rmcp's subtree contains no HTTP, TLS or WebSocket stack.
Every Cargo package added since the baseline commit must have a license
expression built only from the permissive licenses below, or match the exact
reviewed package-specific MIT choice below. `cargo tree` resolves
foreign targets without installing them, so one host covers all release targets.
"""

import argparse
import fnmatch
import json
import re
import subprocess
import tempfile
import tomllib
from pathlib import Path

from build_inchi_helper import RELEASE_TARGETS

ROOT = Path(__file__).resolve().parents[1]
BASELINE = Path("scripts/agent_dependency_baseline.json")
# main before the agent API added any dependency.
BASELINE_COMMIT = "40c8d82a5ed213685a7eedd150a0b7adf6899846"
# preserve_order reorders every json! object; arbitrary_precision changes numbers.
SERDE_JSON_FORBIDDEN = frozenset({"preserve_order", "arbitrary_precision"})
# Reviewed P1 addition: schemars 1.x, which rmcp's `server` requires, enables
# alloc. serde_json gates that code on any(std, alloc), so with std on it is a no-op.
SERDE_JSON_REVIEWED = frozenset({"alloc"})
RMCP_FORBIDDEN = ("*client*", "transport-*", "auth*", "*reqwest*", "*http*", "macros")
# rmcp's `server` always enables this one: tokio io-util and the tokio-util codec
# over caller-supplied streams, with no network or process transport.
RMCP_SERVER_TRANSPORT = "transport-async-rw"
NETWORK_CRATES = frozenset(
    {"hyper", "reqwest", "axum", "tower-http", "rustls", "tokio-tungstenite"}
)
LICENSES = frozenset(
    {
        "MIT",
        "Apache-2.0",
        "Apache-2.0 WITH LLVM-exception",
        "BSD-2-Clause",
        "BSD-3-Clause",
        "ISC",
        "Zlib",
        "Unicode-3.0",
    }
)
JSON_WIDTH = 100
# This publisher explicitly offers either license (COPYING). Select MIT only
# for the reviewed crates.io archive; never generalize this to another OR branch,
# package/version/source, or checksum. Preserve both terms in packaged notices.
# Attribution and source-text hashes: licenses/rust/aho-corasick-1.1.5-mit-choice.json.
AHO_CORASICK_MIT_CHOICE = {
    "name": "aho-corasick",
    "version": "1.1.5",
    "license": "Unlicense OR MIT",
    "source": "registry+https://github.com/rust-lang/crates.io-index",
    "checksum": "c982642fa9e8606056828ee9a8505737230110bb1099153c79efe865c59d12ba",
}


def run(root, *command):
    return subprocess.run(
        [str(part) for part in command], cwd=root, check=True, stdout=subprocess.PIPE, text=True
    ).stdout


def parse_features(text):
    """Map package names to their enabled features in `--format '{p}|{f}'` output."""
    features = {}
    for line in text.splitlines():
        if not line.strip():
            continue
        package, separator, enabled = line.removesuffix(" (*)").rpartition("|")
        if not separator:
            raise ValueError(f"Unexpected cargo tree line: {line!r}")
        name = package.split(" ", 1)[0]
        features.setdefault(name, set()).update(filter(None, enabled.split(",")))
    return features


def production_features(root, target):
    # Without dev edges Cargo also leaves dev-dependency features unresolved.
    return parse_features(
        run(
            root,
            *("cargo", "tree", "--locked", "-p", "reshiki", "-e", "normal,build"),
            *("--target", target, "--prefix", "none", "--format", "{p}|{f}"),
        )
    )


def rmcp_packages(root, target):
    output = run(
        root,
        *("cargo", "tree", "--locked", "-p", "rmcp", "-e", "normal,build"),
        *("--target", target, "--prefix", "none"),
    )
    return {line.split(" ", 1)[0] for line in output.splitlines() if line.strip()}


def license_allowed(expression):
    """Accept only expressions whose every term, in any branch, is a listed license."""
    if not expression:
        return False
    flat = expression.replace("(", " ").replace(")", " ").strip()
    # Cargo still accepts the legacy "MIT/Apache-2.0" form for OR.
    terms = re.split(r"\s+(?:OR|AND)\s+|\s*/\s*", flat)
    return all(" ".join(term.split()) in LICENSES for term in terms)


def allowlist_problems(rmcp_allowed):
    problems = [] if "server" in rmcp_allowed else ["the rmcp allowlist must contain server"]
    problems += [
        f"the rmcp allowlist must not contain {name}"
        for name in sorted(rmcp_allowed)
        if name != RMCP_SERVER_TRANSPORT
        and any(fnmatch.fnmatchcase(name, pattern) for pattern in RMCP_FORBIDDEN)
    ]
    return problems


def feature_problems(target, features, serde_json_allowed, rmcp_allowed):
    serde_json = features.get("serde_json", set())
    problems = [
        f"serde_json enables {name} for {target}; it would change the byte-pinned Codex tool JSON"
        for name in sorted(serde_json & SERDE_JSON_FORBIDDEN)
    ]
    problems += [
        f"serde_json feature {name} for {target} is not in the baseline"
        for name in sorted(serde_json - SERDE_JSON_FORBIDDEN - serde_json_allowed)
    ]
    problems += [
        f"rmcp feature {name} for {target} is not in the reviewed allowlist"
        for name in sorted(features.get("rmcp", set()) - rmcp_allowed)
    ]
    return problems


def network_problems(target, packages):
    return [
        f"rmcp depends on {name} for {target}; the MCP server must stay network-free"
        for name in sorted(packages & NETWORK_CRATES)
    ]


def selected_mit_license(package, locked_packages):
    """Match the explicit MIT choice against both metadata and locked provenance."""
    choice = AHO_CORASICK_MIT_CHOICE
    if any(
        package.get(field) != choice[field] for field in ("name", "version", "license", "source")
    ):
        return False
    return any(
        all(
            locked.get(field) == choice[field]
            for field in ("name", "version", "source", "checksum")
        )
        for locked in locked_packages
    )


def license_problems(metadata, known, locked_packages):
    problems = []
    for package in metadata["packages"]:
        key = f"{package['name']}@{package['version']}"
        if (
            key not in known
            and not license_allowed(package.get("license"))
            and not selected_mit_license(package, locked_packages)
        ):
            problems.append(
                f"{key} has a license outside the allowed list: {package.get('license')!r}"
            )
    return sorted(problems)


def verify(root=ROOT, metadata=None):
    root = Path(root)
    baseline = json.loads((root / BASELINE).read_text(encoding="utf-8"))
    rmcp_allowed = set(baseline["rmcp_features"])
    problems = allowlist_problems(rmcp_allowed)
    for target in RELEASE_TARGETS:
        serde_json_allowed = SERDE_JSON_REVIEWED | set(
            baseline["serde_json_features"].get(target, [])
        )
        features = production_features(root, target)
        problems += feature_problems(target, features, serde_json_allowed, rmcp_allowed)
        problems += network_problems(target, rmcp_packages(root, target))
    if metadata is None:
        metadata = json.loads(run(root, "cargo", "metadata", "--locked", "--format-version", "1"))
    lock = tomllib.loads((root / "Cargo.lock").read_text(encoding="utf-8"))
    problems += license_problems(metadata, set(baseline["packages"]), lock.get("package", []))
    if problems:
        raise ValueError("Agent dependency audit failed:\n" + "\n".join(f"- {p}" for p in problems))


def layout(value, indent="", key=None, comma=""):
    """Lines in the repository's oxfmt JSON style: arrays inline when they fit."""
    head = indent + ("" if key is None else json.dumps(key) + ": ")
    inner = indent + "  "
    if isinstance(value, dict):
        lines = [head + "{"]
        for index, (name, item) in enumerate(value.items()):
            lines += layout(item, inner, name, "," if index < len(value) - 1 else "")
        return [*lines, indent + "}" + comma]
    flat = head + json.dumps(value) + comma
    if not isinstance(value, list) or len(flat) <= JSON_WIDTH:
        return [flat]
    items = [
        inner + json.dumps(item) + ("," if index < len(value) - 1 else "")
        for index, item in enumerate(value)
    ]
    return [head + "[", *items, indent + "]" + comma]


def write_baseline(root=ROOT):
    """Record main's packages and serde_json features, and the current rmcp features."""
    root = Path(root)
    lock = tomllib.loads(run(root, "git", "show", f"{BASELINE_COMMIT}:Cargo.lock"))
    with tempfile.TemporaryDirectory() as temporary:
        worktree = Path(temporary) / "baseline"
        run(root, "git", "worktree", "add", "--detach", worktree, BASELINE_COMMIT)
        try:
            serde_json = {
                target: sorted(production_features(worktree, target).get("serde_json", []))
                for target in RELEASE_TARGETS
            }
        finally:
            run(root, "git", "worktree", "remove", "--force", worktree)
    rmcp = set()
    for target in RELEASE_TARGETS:
        rmcp |= production_features(root, target).get("rmcp", set())
    baseline = {
        "source_commit": BASELINE_COMMIT,
        "packages": sorted(
            {f"{package['name']}@{package['version']}" for package in lock["package"]}
        ),
        "serde_json_features": serde_json,
        "rmcp_features": sorted(rmcp),
    }
    (root / BASELINE).write_text("\n".join(layout(baseline)) + "\n", encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--write-baseline",
        action="store_true",
        help=f"regenerate {BASELINE} for review (maintainers only)",
    )
    args = parser.parse_args()
    if args.write_baseline:
        write_baseline()
    verify()
    print(f"Agent dependencies passed for {len(RELEASE_TARGETS)} release targets.")


if __name__ == "__main__":
    main()
