"""Skip native checks only when every changed file is documentation or web tooling."""

import json
import os
import re
import subprocess
from pathlib import Path

WEB_FILES = {
    "README.md",
    "package.json",
    "bun.lock",
    ".oxfmtrc.json",
    ".github/FUNDING.yml",
    ".github/workflows/docs.yml",
}


def needs_native(paths):
    # Unknown files, empty diffs and failed Git lookups take the full-check path.
    return not paths or any(
        path not in WEB_FILES and not path.startswith(("docs/", "website/")) for path in paths
    )


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
    native = live or needs_native(changed_paths(event_name, event))
    print(f"Native and golden checks: {native}; live RDKit comparisons: {live}")
    with Path(os.environ["GITHUB_OUTPUT"]).open("a", encoding="utf-8") as output:
        output.write(f"native={str(native).lower()}\nlive={str(live).lower()}\n")


if __name__ == "__main__":
    main()
