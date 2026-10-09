"""Rebuild only the original, deterministic Java adapter; never fetch dependencies."""

import hashlib
import os
import shutil
import subprocess
import tempfile
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent
CORE = ROOT / "opsin-core-2.9.0-jar-with-dependencies.jar"
CORE_SHA256 = "627ee5da4af551f9c4d1d766f545eb7cf519a344776e0bb677247a47abac0252"
ADAPTER_SHA256 = "0ea22d92917e852a2666e035de278ef7e162c6350a7a78c2a89bbd31d2b6a388"
if hashlib.sha256(CORE.read_bytes()).hexdigest() != CORE_SHA256:
    raise SystemExit("Pinned OPSIN core checksum mismatch")
compiler = shutil.which("javac")
if not compiler:
    raise SystemExit(
        "Rebuilding the adapter requires a Java 11+ JDK (runtime users need only a JRE)"
    )
environment = {
    k: v
    for k, v in os.environ.items()
    if k not in {"_JAVA_OPTIONS", "JAVA_TOOL_OPTIONS", "JDK_JAVA_OPTIONS", "CLASSPATH"}
}
with tempfile.TemporaryDirectory(prefix="reshiki-opsin-build-") as temporary:
    subprocess.run(
        [
            compiler,
            "--release",
            "11",
            "-encoding",
            "UTF-8",
            "-g:none",
            "-classpath",
            str(CORE),
            "-d",
            temporary,
            str(ROOT / "ReShikiOpsin.java"),
        ],
        check=True,
        env=environment,
        cwd=temporary,
    )
    path = Path(temporary) / "adapter.jar"
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_STORED) as output:
        for source in sorted(Path(temporary).glob("*.class")):
            entry = zipfile.ZipInfo(source.name, (1980, 1, 1, 0, 0, 0))
            entry.external_attr = 0o644 << 16
            entry.create_system = 3
            output.writestr(entry, source.read_bytes())
    data = path.read_bytes()
    digest = hashlib.sha256(data).hexdigest()
    if digest != ADAPTER_SHA256:
        raise SystemExit(
            "Adapter differs from the reviewed pin; use the documented compiler or review and update all adapter checksum pins"
        )
    destination = ROOT / "reshiki-opsin-adapter.jar"
    destination.write_bytes(data)
    print(digest, destination.name)
