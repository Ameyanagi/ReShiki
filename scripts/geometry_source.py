"""Audit the locked Rust geometry dependency and embedded parameter provenance.

This script reads checked-in Cargo pins and, optionally, an existing COSMolKit
checkout. It never fetches sources or compiles the RDKit reference data.
"""

import argparse
import hashlib
import json
import re
import subprocess
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
GEOMETRY_MANIFEST = Path("native/geometry/Cargo.toml")
COSMOLKIT_VERSION = "0.3.0"
COSMOLKIT_REPOSITORY = "https://github.com/Ameyanagi/COSMolKit.git"
RDKIT_VERSION = "2026.03.6"
RDKIT_REVISION = "0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985"
PARAMETER_REFERENCE_VERSION = "2026.03.1"
PARAMETER_REFERENCE_REVISION = "351f8f378f8ad6bbd517980c38896e66bf907af8"
PARAMETER_ROOT = Path("crates/cosmolkit-core/src/chemistry/forcefield/rdkit")
# Exact bytes audited in the pinned COSMolKit checkout, not C++ build inputs.
PARAMETER_SHA256 = {
    "ForceField/MMFF/Params.cpp": "91af3ffd45515712bb787d2c14c4a1c890367fc15e1a71b1d1782a60a0e34b7f",
    "ForceField/UFF/Params.cpp": "c2e3fedb28233258a5277dcfddd0e163c55bd4cc9e83b2e242fefb0cb96f787e",
    "GraphMol/atomic_data.cpp": "7f9cee6e430b60d303a0a7fa9e33c45e5c529ee204f86afeab6a20f68b6b0631",
    "GraphMol/ForceFieldHelpers/CrystalFF/torsionPreferences_v1.in": "8af25175470dbfc82714dcf54115cccd741ef65463096703a67799c4340d98be",
    "GraphMol/ForceFieldHelpers/CrystalFF/torsionPreferences_v2.in": "c1c1ff3d93b9629d1e71502daa02257c9968ca4788478d4970f81e457a37d8e5",
    "GraphMol/ForceFieldHelpers/CrystalFF/torsionPreferences_smallrings.in": "28124c740e9aba1ec835fa9cef8f095258f0b94f5829dda2b995e99bbd1b5efb",
    "GraphMol/ForceFieldHelpers/CrystalFF/torsionPreferences_macrocycles.in": "0a11044bbcdddf842b505e4baa0b4878e1fcc0725fa7c99dd82ff462938eab6e",
}


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def dependency(root=ROOT):
    root = Path(root)
    manifest = tomllib.loads((root / GEOMETRY_MANIFEST).read_text())
    required = manifest["dependencies"]["cosmolkit-core"]
    if (
        not isinstance(required, dict)
        or required.get("version") != "=" + COSMOLKIT_VERSION
        or required.get("git") != COSMOLKIT_REPOSITORY
        or not re.fullmatch(r"[0-9a-f]{40}", required.get("rev", ""))
    ):
        raise ValueError(
            "Geometry Cargo dependency must pin the exact COSMolKit version and Git revision"
        )
    lock = tomllib.loads((root / "Cargo.lock").read_text())
    packages = [p for p in lock["package"] if p["name"] == "cosmolkit-core"]
    revision = required["rev"]
    source = f"git+{COSMOLKIT_REPOSITORY}?rev={revision}#{revision}"
    if (
        len(packages) != 1
        or packages[0]["version"] != COSMOLKIT_VERSION
        or packages[0].get("source") != source
    ):
        raise ValueError("Geometry Cargo dependency does not match the locked COSMolKit revision")
    return dict(
        name="cosmolkit-core",
        version=COSMOLKIT_VERSION,
        repository=COSMOLKIT_REPOSITORY,
        revision=revision,
        source=source,
    )


def metadata(root=ROOT):
    root = Path(root)
    backend = dependency(root)
    return {
        # Retained wire field: this is the independent RDKit reference version.
        "version": RDKIT_VERSION,
        "revision": backend["revision"],
        "backend": backend,
        "implementation": "Rust",
        "parameter_reference": {
            "name": "RDKit",
            "version": PARAMETER_REFERENCE_VERSION,
            "revision": PARAMETER_REFERENCE_REVISION,
            "repository": "https://github.com/rdkit/rdkit",
        },
        "comparison_reference": {
            "name": "RDKit",
            "version": RDKIT_VERSION,
            "revision": RDKIT_REVISION,
            "repository": "https://github.com/rdkit/rdkit",
        },
        "parameter_data": {
            "repository_path": PARAMETER_ROOT.as_posix(),
            "files_sha256": PARAMETER_SHA256,
            "capture": "COSMolKit embedded data; MMFF adds an Id header, legacy torsion-v1 repeats one row",
        },
        "cargo_manifest_sha256": digest(root / GEOMETRY_MANIFEST),
        "cargo_lock_sha256": digest(root / "Cargo.lock"),
        "runtime": "self-process-rust-core",
        "runtime_python": False,
        "runtime_downloads": False,
    }


def verify_parameters(core):
    for name, expected in PARAMETER_SHA256.items():
        if digest(Path(core) / "src/chemistry/forcefield/rdkit" / name) != expected:
            raise ValueError(f"Geometry parameter data changed: {name}")


def verify(root=ROOT, cosmolkit_source=None, cargo_metadata=None):
    root = Path(root)
    result = metadata(root)
    geometry = root / "native/geometry"
    for obsolete in ("build.rs", "cpp", "vendor"):
        if (geometry / obsolete).exists():
            raise ValueError(f"Obsolete native geometry build input remains: {obsolete}")
    if cosmolkit_source is not None:
        source = Path(cosmolkit_source).resolve()
        revision = subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=source, check=True, capture_output=True, text=True
        ).stdout.strip()
        if revision != result["revision"]:
            raise ValueError("COSMolKit checkout does not match the locked geometry revision")
        verify_parameters(source / "crates/cosmolkit-core")
    if cargo_metadata is not None:
        packages = [p for p in cargo_metadata["packages"] if p["name"] == "cosmolkit-core"]
        if (
            len(packages) != 1
            or packages[0]["version"] != COSMOLKIT_VERSION
            or packages[0].get("source") != result["backend"]["source"]
        ):
            raise ValueError("Resolved COSMolKit source does not match the geometry Cargo pin")
        verify_parameters(Path(packages[0]["manifest_path"]).parent)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--cosmolkit-source",
        type=Path,
        help="Audit parameter bytes in an existing checkout; never downloads",
    )
    args = parser.parse_args()
    print(json.dumps(verify(cosmolkit_source=args.cosmolkit_source), indent=2))


if __name__ == "__main__":
    main()
