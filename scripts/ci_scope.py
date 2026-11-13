"""Skip native checks only when every changed file is documentation or web tooling.

Also build the matrices with macOS legs: those run only on stacked `merge/*`
pull requests, pull requests labelled `full-ci`, and non-PR events (main,
nightly, release, manual runs). Other pull requests get Linux and Windows.
"""

import json
import os
import re
import subprocess
from pathlib import Path
from typing import Any

WEB_FILES = {
    "README.md",
    "package.json",
    "bun.lock",
    "bunfig.toml",
    ".github/FUNDING.yml",
    ".github/workflows/docs.yml",
}


def needs_native(paths):
    # Unknown files, empty diffs and failed Git lookups take the full-check path.
    return not paths or any(
        path not in WEB_FILES and not path.startswith(("docs/", "website/")) for path in paths
    )


# The geometry matrix tests crates/ and native/, which never build the app or JS packages.
APP_ONLY_PREFIXES = ("src/", "packages/", "integrations/", "docs/", "website/")


def needs_geometry(paths):
    return not paths or any(
        path not in WEB_FILES and not path.startswith(APP_ONLY_PREFIXES) for path in paths
    )


def needs_macos(event_name, event):
    if event_name != "pull_request":
        return True
    pull = event.get("pull_request", {})
    labels = {label.get("name") for label in pull.get("labels", [])}
    return pull.get("head", {}).get("ref", "").startswith("merge/") or "full-ci" in labels


PYTHON = "3.12.15"
WINDOWS_PYTHON = "cpython-3.12.15-windows-x86_64-none"
NATIVE_ROWS = [
    {"os": "macos-15", "target": "aarch64-apple-darwin", "python": PYTHON},
    {
        "os": "windows-2022",
        "target": "x86_64-pc-windows-msvc",
        "msvc_arch": "x64",
        "python": WINDOWS_PYTHON,
    },
    {"os": "ubuntu-24.04", "target": "x86_64-unknown-linux-gnu", "python": PYTHON},
]
# Matrix rows by job, in the order and key order the job names list them.
MATRICES: dict[str, list[dict[str, Any]]] = {
    "python": [
        {"os": "macos-15", "python": PYTHON},
        {"os": "windows-2022", "python": WINDOWS_PYTHON},
        {"os": "ubuntu-22.04", "python": PYTHON},
    ],
    # checks.yml rust and rust-lint-goldens
    "rust": NATIVE_ROWS,
    "geometry": [
        {"os": "macos-15", "target": "aarch64-apple-darwin"},
        {"os": "macos-15-intel", "target": "x86_64-apple-darwin"},
        {"os": "windows-2022", "target": "x86_64-pc-windows-msvc", "msvc_arch": "x64"},
        {"os": "windows-11-arm", "target": "aarch64-pc-windows-msvc", "msvc_arch": "arm64"},
        {"os": "ubuntu-22.04", "target": "x86_64-unknown-linux-gnu"},
        {"os": "ubuntu-24.04-arm", "target": "aarch64-unknown-linux-gnu"},
    ],
    # python-wheels.yml; the tests' RDKit has no Intel macOS or Windows ARM64 wheel.
    "wheel": [
        {
            "os": "ubuntu-22.04",
            "artifact": "linux-x64",
            "target": "x86_64-unknown-linux-gnu",
            "test": True,
        },
        {
            "os": "ubuntu-24.04-arm",
            "artifact": "linux-arm64",
            "target": "aarch64-unknown-linux-gnu",
            "test": True,
        },
        {
            "os": "macos-15",
            "artifact": "macos-arm64",
            "target": "aarch64-apple-darwin",
            "test": True,
        },
        {"os": "macos-15-intel", "artifact": "macos-x64", "target": "x86_64-apple-darwin"},
        {
            "os": "windows-2022",
            "artifact": "windows-x64",
            "target": "x86_64-pc-windows-msvc",
            "msvc_arch": "x64",
            "python": WINDOWS_PYTHON,
            "test": True,
        },
        {
            "os": "windows-11-arm",
            "artifact": "windows-arm64",
            "target": "aarch64-pc-windows-msvc",
            "msvc_arch": "arm64",
            "python": "cpython-3.12.15-windows-aarch64-none",
        },
    ],
}


def matrices(macos):
    return {
        job: [row for row in rows if macos or not row["os"].startswith("macos")]
        for job, rows in MATRICES.items()
    }


def changed_paths(event_name, event):
    if event_name == "pull_request":
        base = event.get("pull_request", {}).get("base", {}).get("sha", "")
    elif event_name == "push":
        base = event.get("before", "")
    else:
        return None
    if not isinstance(base, str) or not re.fullmatch(r"[0-9a-f]{40}", base) or set(base) == {"0"}:
        return None
    try:
        # A rename out of src/ must still include the deleted source path.
        output = subprocess.check_output(
            ["git", "diff", "--no-renames", "--name-only", "-z", base, "HEAD", "--"]
        )
    except subprocess.CalledProcessError:
        return None
    return [path for path in output.decode("utf-8", errors="surrogateescape").split("\0") if path]


def main():
    event_name = os.environ.get("GITHUB_EVENT_NAME", "")
    event = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text(encoding="utf-8"))
    live = event_name == "schedule" or os.environ.get("CI_LIVE_REFERENCE") == "true"
    paths = changed_paths(event_name, event)
    native = live or needs_native(paths)
    geometry = live or needs_geometry(paths)
    macos = live or needs_macos(event_name, event)
    print(
        f"Native and golden checks: {native}; geometry: {geometry}; live RDKit comparisons: {live}; macOS: {macos}"
    )
    with Path(os.environ["GITHUB_OUTPUT"]).open("a", encoding="utf-8") as output:
        output.write(
            f"native={str(native).lower()}\ngeometry={str(geometry).lower()}\nlive={str(live).lower()}\n"
            f"macos={str(macos).lower()}\nmatrix={json.dumps(matrices(macos))}\n"
        )


if __name__ == "__main__":
    main()
