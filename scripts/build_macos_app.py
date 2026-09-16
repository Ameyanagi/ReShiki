"""Make a development app bundle from the local debug build."""
from pathlib import Path
import plistlib
import shutil
import subprocess

root = Path(__file__).resolve().parents[1]
subprocess.run(["cargo", "build", "--locked"], cwd=root, check=True)
bundle = root / "target/debug/Moruno.app"
executable = bundle / "Contents/MacOS/moruno"
executable.parent.mkdir(parents=True, exist_ok=True)
shutil.copy2(root / "target/debug/moruno", executable)
with (bundle / "Contents/Info.plist").open("wb") as stream:
    plistlib.dump({"CFBundleName": "Moruno", "CFBundleDisplayName": "Moruno",
                  "CFBundleIdentifier": "dev.moruno.editor", "CFBundleExecutable": "moruno",
                  "CFBundlePackageType": "APPL", "CFBundleShortVersionString": "0.1.0",
                  "NSHighResolutionCapable": True, "LSMinimumSystemVersion": "12.0"}, stream)
print(bundle)
