"""Verify a relocated application runs chemistry without Python, uv, or a checkout."""

import argparse
import json
import os
import subprocess
import tempfile
from pathlib import Path


def verify_payload(package):
    """Reject a stale chemistry project or interpreter in a native distribution."""
    for entry in package.rglob("*"):
        relative = entry.relative_to(package)
        # These are required attribution records, not executable dependencies.
        if "Licenses" in relative.parts:
            continue
        name = entry.name.lower()
        if (
            name in {"chemistry", ".venv", "__pycache__", "uv.lock", "pyproject.toml", "pyvenv.cfg"}
            or entry.suffix.lower() in {".py", ".pyc", ".pyo", ".pyd"}
            or name in {"python", "python.exe", "python3", "python3.exe", "uv", "uv.exe"}
            or name.startswith(("libpython", "python3.", "python31", "rdkit"))
        ):
            raise ValueError(f"Python chemistry payload in native package: {relative}")


def verify_runtime(binary, package, *, user_data=None):
    package = Path(package).resolve(strict=True)
    binary = Path(binary).resolve(strict=True)
    if not binary.is_relative_to(package):
        raise ValueError("Runtime check executable must belong to the package")
    verify_payload(package)
    with tempfile.TemporaryDirectory(prefix="ReShiki native runtime ") as temporary:
        root = Path(temporary)
        empty_path = root / "Empty PATH"
        empty_path.mkdir()
        environment = dict(os.environ)
        for key in (
            "RESHIKI_INCHI_HELPER",
            "MORUNO_INCHI_HELPER",
            "VIRTUAL_ENV",
            "PYTHONPATH",
            "PYTHONHOME",
            "UV_PROJECT_ENVIRONMENT",
        ):
            environment.pop(key, None)
        environment.update(
            PATH=str(empty_path),
            HOME=str(root / "Home"),
            USERPROFILE=str(root / "Home"),
            XDG_CACHE_HOME=str(root / "Cache"),
            XDG_DATA_HOME=str(root / "Data"),
            APPDATA=str(root / "Roaming"),
            LOCALAPPDATA=str(root / "Local"),
            UV_CACHE_DIR=str(root / "UV cache"),
            UV_OFFLINE="1",
            PYTHONNOUSERSITE="1",
        )
        for prefix in ("RESHIKI", "MORUNO"):
            for key, value in {
                "PYTHON": root / "Missing Python",
                "REFERENCE_PYTHON": root / "Missing reference Python",
                "UV": root / "Missing uv",
                "ROOT": root / "No checkout",
                "RUNTIME_DIR": root / "Chemistry runtime",
                "DATA_DIR": user_data if user_data is not None else root / "User data",
            }.items():
                environment[f"{prefix}_{key}"] = str(value)
        # Both launches must run from the shipped binaries. UV_OFFLINE is an extra
        # guard against uv downloads, not a claim that this is a network sandbox.
        for launch in range(2):
            response = subprocess.run(
                [str(binary), "--engine-check"],
                cwd=root,
                env=environment,
                capture_output=True,
                text=True,
                timeout=120,
                check=True,
            )
            analysis = json.loads(response.stdout).get("analysis", {})
            if analysis.get("formula") != "C2H6O" or analysis.get("smiles") != "CCO":
                raise ValueError(
                    f"Native chemistry check did not return ethanol (launch {launch + 1})"
                )
            if (root / "Chemistry runtime").exists() or (root / "UV cache").exists():
                raise ValueError("Native chemistry check created a Python chemistry environment")
            verify_payload(root)
        verify_payload(package)
    print("Native chemistry passed twice with Python, uv, and the checkout unavailable.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package", type=Path, required=True)
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args()
    verify_runtime(args.binary, args.package)


if __name__ == "__main__":
    main()
