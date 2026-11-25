"""Audit Cargo dependencies for license, network and serde_json drift.

For every release target, on normal and build edges only (dev-dependencies never
ship, so the interop test's rmcp `client` feature is out of scope):
- serde_json features stay within the baseline and never include preserve_order
  or arbitrary_precision, which would change the byte-pinned Codex tool JSON;
- rmcp features stay within the reviewed server-only allowlist;
- rmcp's subtree contains no HTTP, TLS or WebSocket stack.
Every Cargo package added since the baseline commit must have a license
expression built only from the permissive licenses below, or match an exact
reviewed package-specific license decision below. The MIT crates that generate
code from LGPL KDE protocol files must match their reviewed version and archive. `cargo tree` resolves
foreign targets without installing them, so one host covers all release targets.

The xTB audit rejects copyleft licenses and the axtb fork in default production
graphs for release targets and WebAssembly. It requires exactly the pinned LGPL
fork crates as the copyleft packages in macOS and Linux xTB builds. All other
xTB dependencies face the existing license check. The lock, worker revision,
source notice and upstream license files must agree.
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
from license_notices import KDE_PROTOCOL_CRATES, KDE_PROTOCOL_REVIEW

ROOT = Path(__file__).resolve().parents[1]
BASELINE = Path("scripts/agent_dependency_baseline.json")
# main before the agent API added any dependency.
BASELINE_COMMIT = "40c8d82a5ed213685a7eedd150a0b7adf6899846"
# preserve_order reorders every json! object; arbitrary_precision changes numbers.
SERDE_JSON_FORBIDDEN = frozenset({"preserve_order", "arbitrary_precision"})
# Reviewed P1 addition: schemars 1.x, which rmcp's `server` requires, enables
# alloc. serde_json gates that code on any(std, alloc), so with std on it is a no-op.
# cosmolkit-conformer 0.5.0 enables raw_value, which only adds the opt-in
# RawValue type; Value, json! and number output are unchanged.
SERDE_JSON_REVIEWED = frozenset({"alloc", "raw_value"})
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
        "Python-2.0",
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
# The certificate data may be redistributed with the complete agreement, which
# license_notices retains. Review only this published archive, not every CDLA
# package or version. Evidence: licenses/rust/webpki-root-certs-1.0.10-cdla-review.json.
WEBPKI_ROOT_CERTS_CDLA_REVIEW = {
    "name": "webpki-root-certs",
    "version": "1.0.10",
    "license": "CDLA-Permissive-2.0",
    "source": "registry+https://github.com/rust-lang/crates.io-index",
    "checksum": "58ec80e73fd8baf57bf0d9615554c75348bda990d6d5bbeab964dd068319fecb",
}
AXTB_SOURCE = "https://github.com/Ameyanagi/axtb"
AXTB_LICENSE = "LGPL-3.0-or-later"
AXTB_PACKAGES = frozenset(
    {"axtb", "axtb-base", "axtb-d4", "axtb-driver", "axtb-gfnff", "axtb-gxtb", "axtb-solvation"}
)


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


def production_features(root, target, *features):
    # Without dev edges Cargo also leaves dev-dependency features unresolved.
    return parse_features(
        run(
            root,
            *("cargo", "tree", "--locked", "-p", "reshiki", *features, "-e", "normal,build"),
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


def license_terms(expression):
    flat = expression.replace("(", " ").replace(")", " ").strip()
    # Cargo still accepts the legacy "MIT/Apache-2.0" form for OR.
    terms = re.split(r"\s+(?:OR|AND)\s+|\s*/\s*", flat)
    return [" ".join(term.split()) for term in terms]


def license_allowed(expression):
    """Accept only expressions whose every term, in any branch, is a listed license."""
    return bool(expression) and all(term in LICENSES for term in license_terms(expression))


def copyleft(expression):
    """Whether a GPL-family license (GPL, LGPL, AGPL, any case) binds the package.

    SPDX precedence: WITH, then AND, then OR (Cargo's legacy `/` is OR). An OR
    binds only if every alternative does, so `Apache-2.0 OR GPL-2.0-only`
    (self_cell) is used under Apache-2.0; an AND binds if any part does.
    """
    tokens = re.findall(r"\(|\)|/|[^\s()/]+", expression or "")
    position = 0

    def peek():
        return tokens[position].upper() if position < len(tokens) else None

    def term():
        nonlocal position
        if peek() == "(":
            position += 1
            binding = either()
            position += 1  # ")"
        else:
            binding = bool(re.match(r"(?:A|L)?GPL(?:[-+]|$)", tokens[position], re.I))
            position += 1
        if peek() == "WITH":
            position += 2
        return binding

    def both():
        nonlocal position
        binding = term()
        while peek() == "AND":
            position += 1
            binding = term() or binding
        return binding

    def either():
        nonlocal position
        binding = both()
        while peek() in {"OR", "/"}:
            position += 1
            binding = both() and binding
        return binding

    return bool(tokens) and either()


def license_tree(root, target, *packages):
    output = run(
        root,
        "cargo",
        "tree",
        "--locked",
        *packages,
        "-e",
        "normal,build",
        "--target",
        target,
        "--prefix",
        "none",
        "--format",
        "{p}|{l}",
    )
    entries = set()
    for line in output.splitlines():
        if not line.strip():
            continue
        package, separator, license = line.removesuffix(" (*)").rpartition("|")
        # Cargo prints `name vX (proc-macro) (source)`; accept either order.
        identity = re.fullmatch(
            r"(\S+) v(\S+)(?: \((.*)\))?",
            package.replace(" (proc-macro)", ""),
        )
        if not separator or not identity:
            raise ValueError(f"Unexpected cargo tree line: {line!r}")
        name, version, source = identity.groups(default="")
        entries.add((name, version, source, license))
    return sorted(entries)


def axtb_fork(source):
    return re.split(r"[?#]", source.removeprefix("git+"), maxsplit=1)[0] == AXTB_SOURCE


def default_copyleft_problems(target, entries):
    problems = []
    for name, version, source, license in entries:
        if axtb_fork(source):
            reason = f"comes from the axtb fork ({source})"
        elif copyleft(license):
            reason = f"declares a copyleft license ({license})"
        else:
            continue
        problems.append(f"xTB rule 1: default graph for {target}: {name}@{version} {reason}")
    return problems


def xtb_copyleft_problems(target, entries, revision):
    problems = []
    found = set()
    prefix = f"xTB rule 2: xtb graph for {target}:"
    for name, version, source, license in entries:
        if name in AXTB_PACKAGES and axtb_fork(source):
            found.add(name)
            url, _, commit = source.partition("#")
            # Cargo tree shows the full rev query and an abbreviated commit fragment.
            if (
                url != f"{AXTB_SOURCE}?rev={revision}"
                or len(commit) < 7
                or not revision.startswith(commit)
            ):
                problems.append(f"{prefix} {name}@{version} has the wrong pinned source ({source})")
            if license != AXTB_LICENSE:
                problems.append(
                    f"{prefix} {name}@{version} must declare {AXTB_LICENSE}, got {license!r}"
                )
        elif copyleft(license):
            problems.append(f"{prefix} unexpected copyleft package {name}@{version} ({license})")
    missing = AXTB_PACKAGES - found
    if missing:
        problems.append(f"{prefix} missing axtb fork packages: {', '.join(sorted(missing))}")
    return problems


def axtb_revision(root):
    worker = root / "crates/xtb-worker/src/lib.rs"
    revision = re.search(
        r'^pub const AXTB_REV: &str = "([0-9a-f]{40})";$', worker.read_text(encoding="utf-8"), re.M
    )
    if not revision:
        raise ValueError(f"Missing or invalid AXTB_REV in {worker.relative_to(root)}")
    return revision[1]


def axtb_locked_source(revision):
    return f"git+{AXTB_SOURCE}?rev={revision}#{revision}"


def pin_problems(root, locked_packages, xtb_metadata, revision):
    expected_source = axtb_locked_source(revision)
    problems = []
    # The two gemm patches come from the fork too and must move with the pin.
    for name in ("axtb-driver", "gemm-common", "private-gemm-x86"):
        sources = [
            package.get("source")
            for package in locked_packages
            if package["name"] == name and axtb_fork(package.get("source") or "")
        ]
        if sources != [expected_source]:
            problems.append(f"Cargo.lock {name} source must be the fork at AXTB_REV ({revision})")
    notices = root / "licenses/axtb"
    source = (notices / "SOURCE").read_text(encoding="utf-8")
    for field, value in (("Revision", revision), ("Fork", f"{AXTB_SOURCE}, branch reshiki")):
        if re.findall(rf"^{field}:[ \t]*(.+?)[ \t]*$", source, re.M) != [value]:
            problems.append(f"licenses/axtb/SOURCE must have one {field} line: {value}")

    driver = next(
        (
            package
            for package in xtb_metadata["packages"]
            if package["name"] == "axtb-driver" and package.get("source") == expected_source
        ),
        None,
    )
    if driver is None:
        problems.append("xtb metadata is missing axtb-driver at the pinned revision")
    else:
        upstream = Path(driver["manifest_path"]).parent
        for name in ("COPYING", "COPYING.LESSER", "NOTICE"):
            if (notices / name).read_bytes() != (upstream / name).read_bytes():
                problems.append(f"licenses/axtb/{name} must match the pinned fork byte for byte")
        # The fork's notice of modification lives at its repository root, which
        # only a git checkout has; vendored layouts (Nix, the source archive)
        # carry the crate directories alone.
        changes = upstream.parents[1] / "CHANGES-reshiki.md"
        if changes.is_file() and (notices / changes.name).read_bytes() != changes.read_bytes():
            problems.append(
                "licenses/axtb/CHANGES-reshiki.md must match the pinned fork byte for byte"
            )
    return problems


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


def reviewed_certificate_data_license(package, locked_packages):
    """Accept only the exact published certificate-data archive reviewed for CDLA."""
    reviewed = WEBPKI_ROOT_CERTS_CDLA_REVIEW
    if any(
        package.get(field) != reviewed[field] for field in ("name", "version", "license", "source")
    ):
        return False
    return any(
        all(
            locked.get(field) == reviewed[field]
            for field in ("name", "version", "source", "checksum")
        )
        for locked in locked_packages
    )


def kde_protocol_problems(root, locked_packages):
    """Flag any locked KDE-protocol crate other than the reviewed versions and archives."""
    reviewed = json.loads((root / KDE_PROTOCOL_REVIEW).read_text(encoding="utf-8"))["packages"]
    return sorted(
        f"{key} generates code from LGPL KDE protocol files; review {KDE_PROTOCOL_REVIEW}"
        for locked in locked_packages
        if locked.get("name") in KDE_PROTOCOL_CRATES
        for key in [f"{locked['name']}@{locked.get('version')}"]
        if key not in reviewed
        or any(reviewed[key][field] != locked.get(field) for field in ("source", "checksum"))
    )


def license_problems(metadata, known, locked_packages):
    problems = []
    for package in metadata["packages"]:
        key = f"{package['name']}@{package['version']}"
        if (
            key not in known
            and not license_allowed(package.get("license"))
            and not selected_mit_license(package, locked_packages)
            and not reviewed_certificate_data_license(package, locked_packages)
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
        # The xtb build must not drift either (macOS and Linux releases).
        if RELEASE_TARGETS[target][0] in {"macos", "linux"}:
            features = production_features(root, target, "--features", "xtb")
            problems += feature_problems(
                f"{target} with xtb", features, serde_json_allowed, rmcp_allowed
            )
    if metadata is None:
        metadata = json.loads(run(root, "cargo", "metadata", "--locked", "--format-version", "1"))
    lock = tomllib.loads((root / "Cargo.lock").read_text(encoding="utf-8"))
    known = set(baseline["packages"])
    locked_packages = lock.get("package", [])
    problems += license_problems(metadata, known, locked_packages)
    problems += kde_protocol_problems(root, locked_packages)

    revision = axtb_revision(root)
    for target in (*RELEASE_TARGETS, "wasm32-unknown-unknown"):
        problems += default_copyleft_problems(target, license_tree(root, target, "--workspace"))
        if target in RELEASE_TARGETS and RELEASE_TARGETS[target][0] in {"macos", "linux"}:
            entries = license_tree(root, target, "-p", "reshiki", "--features", "xtb")
            problems += xtb_copyleft_problems(target, entries, revision)

    xtb_metadata = json.loads(
        run(root, "cargo", "metadata", "--locked", "--format-version", "1", "--features", "xtb")
    )
    problems += pin_problems(root, locked_packages, xtb_metadata, revision)
    expected_source = axtb_locked_source(revision)
    other_packages = [
        package
        for package in xtb_metadata["packages"]
        if not (package["name"] in AXTB_PACKAGES and package.get("source") == expected_source)
    ]
    problems += [
        f"xTB rule 3: {problem}"
        for problem in license_problems({"packages": other_packages}, known, locked_packages)
    ]
    if problems:
        raise ValueError("Agent dependency audit failed:\n" + "\n".join(f"- {p}" for p in problems))


def layout(value, indent="", key=None, comma=""):
    """Lines in the repository's `bun fmt` JSON style: arrays inline when they fit."""
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
