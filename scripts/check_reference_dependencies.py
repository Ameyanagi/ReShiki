"""Check the independent RDKit reference with its minimal Python dependencies."""

import os
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def main():
    with tempfile.TemporaryDirectory(prefix="reshiki-reference-") as directory:
        environment = Path(directory) / "venv"
        env: dict[str, str] = {
            **os.environ,
            "UV_PROJECT_ENVIRONMENT": str(environment),
            "PYTHONNOUSERSITE": "1",
        }
        env.pop("VIRTUAL_ENV", None)
        env.pop("PYTHONPATH", None)
        subprocess.run(
            ["uv", "sync", "--locked", "--no-dev", "--python", sys.executable],
            cwd=ROOT,
            env=env,
            check=True,
            timeout=180,
        )
        python = environment / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
        subprocess.run(
            [
                str(python),
                "-c",
                "import importlib.util; assert importlib.util.find_spec('PIL') is None, 'Pillow installed in the minimal reference environment'",
            ],
            cwd=ROOT,
            env=env,
            check=True,
            timeout=30,
        )
        subprocess.run(
            [
                str(python),
                "-m",
                "unittest",
                "tests.test_engine",
                "tests.test_local_properties",
                "tests.test_local_pictures",
                "tests.test_prepared",
            ],
            cwd=ROOT,
            env=env,
            check=True,
            timeout=180,
        )
    print("Reference chemistry and picture transport passed without Pillow")


if __name__ == "__main__":
    main()
