"""Regenerate the OPSIN DFA table and every hash that pins it from regexTokens.xml.

Run after editing crates/nazuchem-n2s/resources/xml/regexTokens.xml, or after a merge
conflict in the generated files: merge regexTokens.xml and any prose by hand,
then rerun. Conflict hunks that differ only in hashes are resolved here, and
git keeps one side of automata.bin. --check verifies without writing.
"""

import argparse
import hashlib
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CRATE = ROOT / "crates/nazuchem-n2s"
MANIFEST = CRATE / "provenance/manifest.json"
AUTOMATA = CRATE / "resources/automata.bin"
OVERRIDE = "resources/uk/ac/cam/ch/wwmm/opsin/resources/regexTokens.xml"
JAR_SHA256 = "931be2952b290dea62548a3bb4129703078c763576a836ac99d8a54d30083f03"
HEX = "[0-9a-f]{64}"
CONFLICT = re.compile(r"<<<<<<< .*?\n(.*?)(?:\|{7} .*?\n.*?)?=======\n(.*?)>>>>>>> .*?\n", re.S)


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def export_dfa(jar, javac, java, out):
    """Run the recorded exporter with the checked regexTokens.xml as resource override."""
    with tempfile.TemporaryDirectory() as tmp:
        classes, work = Path(tmp, "classes"), Path(tmp, "work")
        classes.mkdir()
        (work / OVERRIDE).parent.mkdir(parents=True)
        shutil.copy(CRATE / "provenance/ExportDfa.java.txt", classes / "ExportDfa.java")
        shutil.copy(CRATE / "resources/xml/regexTokens.xml", work / OVERRIDE)
        subprocess.run([javac, "-cp", jar, "-d", classes, classes / "ExportDfa.java"], check=True)
        subprocess.run(
            [
                java,
                "-cp",
                f"{jar}{os.pathsep}{classes}",
                "uk.ac.cam.ch.wwmm.opsin.ExportDfa",
                out,
            ],
            cwd=work,
            check=True,
        )


def read(path):
    """Read `path`, taking our side of every merge conflict that differs only in hashes."""

    def resolve(hunk):
        ours, theirs = hunk.groups()
        return ours if re.sub(HEX, "", ours) == re.sub(HEX, "", theirs) else hunk[0]

    text = CONFLICT.sub(resolve, path.read_bytes().decode())
    if "<<<<<<< " in text:
        raise SystemExit(f"Resolve the remaining merge conflicts in {path.relative_to(ROOT)}.")
    return text


def pin(text, pattern, value, path):
    """Replace the one hash that follows `pattern` (a regex prefix) in `text`."""
    text, count = re.subn(f"({pattern}){HEX}", rf"\g<1>{value}", text)
    if count != 1:
        raise SystemExit(f"{path.relative_to(ROOT)}: expected one hash after {pattern!r}.")
    return text


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--jar",
        type=Path,
        default=os.environ.get("OPSIN_JAR"),
        help="pinned opsin-core-3.0-SNAPSHOT-jar-with-dependencies.jar (default: $OPSIN_JAR)",
    )
    parser.add_argument(
        "--java-home",
        type=Path,
        default=os.environ.get("JAVA_HOME"),
        help="JDK 21 home containing bin/javac (default: $JAVA_HOME)",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="fail if any generated file is stale instead of writing",
    )
    args = parser.parse_args()

    if args.jar is None or not args.jar.is_file():
        raise SystemExit(
            "Pass --jar or set OPSIN_JAR to the pinned OPSIN development oracle "
            f"opsin-core-3.0-SNAPSHOT-jar-with-dependencies.jar (SHA256 {JAR_SHA256}); "
            "see crates/nazuchem-n2s/provenance/README.md."
        )
    if (digest := sha256(args.jar.read_bytes())) != JAR_SHA256:
        raise SystemExit(f"{args.jar} has SHA256 {digest}, not the pinned {JAR_SHA256}.")
    bin_dir = args.java_home and args.java_home.resolve() / "bin"
    javac, java = (bin_dir and shutil.which(tool, path=bin_dir) for tool in ("javac", "java"))
    if not (javac and java):
        raise SystemExit("Pass --java-home or set JAVA_HOME to a JDK 21 home with bin/javac.")

    # Read every text file first, so unresolved conflicts fail before the slow export.
    read(CRATE / "resources/xml/regexTokens.xml")
    manifest = read(MANIFEST)
    pinned = [
        (path, read(path), pattern)
        for path, pattern in [
            (CRATE / "src/lib.rs", r'RESOURCE_FINGERPRINT: &str =\s*"'),
            (CRATE / "provenance/README.md", r"exported crate constant is\s*`"),
            (ROOT / "src/naming/catalog/provenance.json", r'\n    "resource_fingerprint": "'),
        ]
    ]
    with tempfile.TemporaryDirectory() as tmp:
        table = Path(tmp, "automata.bin")
        export_dfa(args.jar.resolve(), javac, java, table)
        generated = {AUTOMATA: table.read_bytes()}

    # Hash every resource listed in the manifest, then the fingerprint over them in
    # sorted path order: path, NUL, raw bytes, NUL.
    fingerprint = hashlib.sha256()
    for path in sorted(re.findall(rf'"(resources/[^"]+)": "{HEX}"', manifest)):
        data = generated.get(CRATE / path) or (CRATE / path).read_bytes()
        manifest = pin(manifest, f'"{re.escape(path)}": "', sha256(data), MANIFEST)
        fingerprint.update(path.encode() + b"\0" + data + b"\0")
    fingerprint = fingerprint.hexdigest()
    manifest = pin(manifest, r'"resource_fingerprint": \{[^}]*"sha256": "', fingerprint, MANIFEST)
    generated[MANIFEST] = manifest.encode()
    for path, text, pattern in pinned:
        generated[path] = pin(text, pattern, fingerprint, path).encode()

    stale = [
        path
        for path, content in generated.items()
        if not path.exists() or path.read_bytes() != content
    ]
    for path in stale:
        print(f"{'stale' if args.check else 'updated'}: {path.relative_to(ROOT)}")
        if not args.check:
            path.write_bytes(generated[path])
    print(f"resource fingerprint {fingerprint}")
    if args.check and stale:
        sys.exit("Generated OPSIN grammar files are stale; rerun without --check.")


if __name__ == "__main__":
    main()
