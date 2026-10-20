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
COSMOLKIT_VERSION = "0.5.0"
COSMOLKIT_REPOSITORY = "https://github.com/Ameyanagi/COSMolKit.git"
RDKIT_VERSION = "2026.03.6"
RDKIT_REVISION = "0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985"
# COSMolKit records this RDKit commit as the source of its embedded tables
# (crates/cosmolkit-forcefields/src/mmff/parameter-provenance.md).
PARAMETER_REFERENCE_VERSION = "2026.03.1"
PARAMETER_REFERENCE_REVISION = "351f8f378f8ad6bbd517980c38896e66bf907af8"
# The geometry facade and the crates whose embedded data it consumes.
COSMOLKIT_CRATES = ("cosmolkit", "cosmolkit-core", "cosmolkit-forcefields")
# Exact bytes audited in the pinned COSMolKit checkout, keyed by crate-relative
# path: MMFF, UFF and CrystalFF tables and the periodic table rows.
PARAMETER_SHA256 = {
    "cosmolkit-core/src/periodic_table.rs": "951d98267f4d9974499aec41b9499d8db2eaec12878b274863bf33feb9a02106",
    "cosmolkit-core/src/periodic_table_isotopes.tsv": "18bb0cfa9c75a7e451f98aa62a6ab584e65dccb004ae167afb8f175bbddfd410",
    "cosmolkit-forcefields/src/crystalff/params/macrocycles.txt": "2068befd9d50344cfdc4924304ea4eb83ccae00c97915d93c13e0e20fe34b5b5",
    "cosmolkit-forcefields/src/crystalff/params/smallrings.txt": "73f833deacf5bad450cc2df2e847a8f731462eaeba2f21fd2c45a55e6cd33b61",
    "cosmolkit-forcefields/src/crystalff/params/v1.txt": "368e03e7702fd51bea155213766292a14d258b4bab058999a5741b79fe176cdc",
    "cosmolkit-forcefields/src/crystalff/params/v2.txt": "f908fe1ae0d8d228c6729173dfec289dfab68e43e0468968c3f9986f08c5790d",
    "cosmolkit-forcefields/src/mmff/default_angle.tsv": "53e6e385eba67d020e7fe00f9c9520181f6c964d1f72827a638d66825f483b70",
    "cosmolkit-forcefields/src/mmff/default_bndk.tsv": "b8767ef084b90c0c99c670ff2b2100765713522951fcd82f61c42b57d7bc5955",
    "cosmolkit-forcefields/src/mmff/default_bond.tsv": "2d6aaa670c425f98ec9a8b18a3cd6e5aef50f14a312ae8d92e0ad7fa79caa2bb",
    "cosmolkit-forcefields/src/mmff/default_chg.tsv": "914c03dbbaf053c0adb00340da4812f9212dd79c1c98bfc67bd5f9391be70e6b",
    "cosmolkit-forcefields/src/mmff/default_cov_rad_pau_ele.tsv": "ad2f757dd038bb1b5161db003deb2edc6fa1a5055991d4d89f35e3fdcea6fd48",
    "cosmolkit-forcefields/src/mmff/default_def.tsv": "9febef6c21516bc67b6176176eb3131809345baa425b9ca1e25e4398a51be24e",
    "cosmolkit-forcefields/src/mmff/default_dfsb.tsv": "e4150c4719c15b569e779306c7d4647f9f14f949c4712af39ea67a1e458ddf55",
    "cosmolkit-forcefields/src/mmff/default_herschbach_laurie.tsv": "41dcb5b0d58e14e9aa1cf8d04340a0a4006273bd273c7fc6ed7d674d07b457d3",
    "cosmolkit-forcefields/src/mmff/default_oop.tsv": "97501718884333a9fe35460d025376f5ebd7f0644cf29207a7db6a078e372bf9",
    "cosmolkit-forcefields/src/mmff/default_oop_s.tsv": "848807b023cc31a21ababb779b934abff0af8547e5ce8a6d2f85bc094b843e32",
    "cosmolkit-forcefields/src/mmff/default_pbci.tsv": "65b1aa8df49edec87ca37f7a1474d61e6933ac7b66e5384e44162377b24859f9",
    "cosmolkit-forcefields/src/mmff/default_prop.tsv": "7ddda181552effed6fbd78e9ee9a70c4d54fe0e8850b29de5d8ffb8d4cda9b2f",
    "cosmolkit-forcefields/src/mmff/default_stbn.tsv": "3d54fe7c56108ccbbe7c06dc93adb6b911acb4495416f7ff4faa4bc4a73fc5b4",
    "cosmolkit-forcefields/src/mmff/default_tor.tsv": "fec53b928fee6cf489ec6dd3e73697f626b944ef4c9d76d8841397ec8c0917a6",
    "cosmolkit-forcefields/src/mmff/default_tor_s.tsv": "b5f2d7588930ee193712c35012e54cfd83b5d327b3a3f52604cc26556a37576a",
    "cosmolkit-forcefields/src/mmff/default_vdw.tsv": "c9d60e3dbb91587a02314055f64b03169667d042eabce9d6e033a4f0b87aa7ca",
    "cosmolkit-forcefields/src/uff/default_params.tsv": "4f2e2d5a29fd53de05e3b13ce84e5d0313037a30d5dba69bff663de450fcbd73",
}


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def dependency(root=ROOT):
    root = Path(root)
    manifest = tomllib.loads((root / GEOMETRY_MANIFEST).read_text())
    revisions = set()
    for name in ("cosmolkit", "cosmolkit-core"):
        required = manifest["dependencies"].get(name)
        if (
            not isinstance(required, dict)
            or required.get("version") != "=" + COSMOLKIT_VERSION
            or required.get("git") != COSMOLKIT_REPOSITORY
            or not re.fullmatch(r"[0-9a-f]{40}", required.get("rev", ""))
        ):
            raise ValueError(
                "Geometry Cargo dependency must pin the exact COSMolKit version and Git revision"
            )
        revisions.add(required["rev"])
    if len(revisions) != 1:
        raise ValueError("Geometry Cargo dependencies must pin one COSMolKit revision")
    revision = revisions.pop()
    source = f"git+{COSMOLKIT_REPOSITORY}?rev={revision}#{revision}"
    lock = tomllib.loads((root / "Cargo.lock").read_text())
    for name in COSMOLKIT_CRATES:
        packages = [p for p in lock["package"] if p["name"] == name]
        if (
            len(packages) != 1
            or packages[0]["version"] != COSMOLKIT_VERSION
            or packages[0].get("source") != source
        ):
            raise ValueError(
                "Geometry Cargo dependency does not match the locked COSMolKit revision"
            )
    return dict(
        name="cosmolkit",
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
            "repository_path": "crates",
            "files_sha256": PARAMETER_SHA256,
            "capture": "COSMolKit embedded tables captured from the RDKit parameter sources",
        },
        "cargo_manifest_sha256": digest(root / GEOMETRY_MANIFEST),
        "cargo_lock_sha256": digest(root / "Cargo.lock"),
        "runtime": "self-process-rust-core",
        "runtime_python": False,
        "runtime_downloads": False,
    }


def verify_parameters(crates):
    """Check audited bytes; crates maps each crate name to its directory."""
    for name, expected in PARAMETER_SHA256.items():
        crate, relative = name.split("/", 1)
        if digest(Path(crates[crate]) / relative) != expected:
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
        verify_parameters({name: source / "crates" / name for name in COSMOLKIT_CRATES})
    if cargo_metadata is not None:
        crates = {}
        for name in {name.split("/", 1)[0] for name in PARAMETER_SHA256}:
            packages = [p for p in cargo_metadata["packages"] if p["name"] == name]
            if (
                len(packages) != 1
                or packages[0]["version"] != COSMOLKIT_VERSION
                or packages[0].get("source") != result["backend"]["source"]
            ):
                raise ValueError("Resolved COSMolKit source does not match the geometry Cargo pin")
            crates[name] = Path(packages[0]["manifest_path"]).parent
        verify_parameters(crates)
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
