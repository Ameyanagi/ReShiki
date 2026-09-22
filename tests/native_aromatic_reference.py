"""Explicitly capture full aromatic responses using only the pinned Python worker.

Use --corpus for independent inputs, --serialize for the opt-in request-only
Rust test, then --requests for original worker responses. Worker results are
created only by Python/RDKit; fixed preflight diagnostics come from the original
Rust reference boundary. Replay never calls this tool or updates a fixture.
"""

import argparse
import copy
import gzip
import hashlib
import importlib.metadata
import io
import json
import os
import platform
import re
import subprocess
import sys
import tempfile
from pathlib import Path

from rdkit import RDConfig, RDLogger, rdBase

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from engine.worker import handle

VERSION = "2026.03.6"
SOURCE = "0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985"


def encoded(value):
    return json.dumps(
        value, ensure_ascii=True, allow_nan=False, sort_keys=True, separators=(",", ":")
    )


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def provenance():
    sources = sorted(
        [*ROOT.joinpath("engine").rglob("*.py"), *ROOT.joinpath("engine").rglob("*.json")]
    )
    sources += [
        ROOT / "assets/templates.json",
        ROOT / "tests/native_aromatic_reference.py",
        ROOT / "tests/aromatic_display_reference.py",
        ROOT / "tests/document_preparation_reference.py",
        ROOT / "tests/perception_reference.py",
        ROOT / "tests/valence_reference.py",
        ROOT / "tests/kekulize_reference.py",
        ROOT / "tests/stereo_reference.py",
        ROOT / "tests/ranking_reference.py",
    ]
    package = importlib.metadata.distribution("rdkit")
    libraries = {}
    for item in package.files or []:
        name = str(item)
        if any(
            part in name for part in ("GraphMol", "SmilesParse", "Depictor", "RDGeneral", "Inchi")
        ) and name.endswith((".so.1", ".so", ".dylib", ".dll", ".pyd")):
            libraries[name] = digest(Path(package.locate_file(item)))
    return dict(
        format_version=1,
        rdkit_version=rdBase.rdkitVersion,
        rdkit_source=SOURCE,
        rdkit_build=rdBase.rdkitBuild,
        python=platform.python_version(),
        system=platform.system(),
        machine=platform.machine(),
        source_sha256={p.relative_to(ROOT).as_posix(): digest(p) for p in sources},
        nci_sha256=digest(Path(RDConfig.RDDataDir) / "NCI/first_5K.smi"),
        native_sha256=libraries,
    )


def requests(path):
    if path:
        with gzip.open(path, "rt", encoding="utf-8") as stream:
            header = json.loads(next(stream))
            if header["rdkit_version"] != VERSION:
                raise ValueError("Request fixture version differs")
            for line in stream:
                record = json.loads(line)
                yield record["name"], record["request"], record["preflight_error"]
        return
    child = subprocess.Popen(
        [sys.executable, str(ROOT / "tests/aromatic_display_reference.py")],
        stdout=subprocess.PIPE,
        encoding="utf-8",
    )
    try:
        if child.stdout is None:
            raise RuntimeError("Missing independent corpus")
        header = json.loads(next(child.stdout))
        if header["rdkit_version"] != VERSION:
            raise ValueError("Corpus version differs")
        for line in child.stdout:
            case = json.loads(line)
            yield (
                case["name"],
                dict(
                    protocol=1,
                    operation="aromatic",
                    document=case["document"],
                    selected_ids=case["selection"],
                ),
                None,
            )
        if child.wait() != 0:
            raise RuntimeError("Independent corpus failed")
    finally:
        if child.poll() is None:
            child.kill()
        child.wait()
        if child.stdout is not None:
            child.stdout.close()


def serialize_requests(source, output):
    with gzip.open(source, "rt", encoding="utf-8") as stream:
        header = json.loads(next(stream))
    count = header.get("cases", 0)
    if count <= 5500 or header.get("request_encoding") != "raw-independent-corpus-v1":
        raise ValueError("Expected a complete independent request corpus")
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="aromatic-requests-", dir=output.parent) as folder:
        fresh = Path(folder).resolve() / "requests.json.gz"
        environment = dict(os.environ)
        environment.update(
            RESHIKI_AROMATIC_REQUESTS_INPUT=str(source.resolve()),
            RESHIKI_AROMATIC_REQUESTS_OUTPUT=str(fresh),
        )
        completed = subprocess.run(
            [
                "cargo",
                "+1.95.0",
                "test",
                "--locked",
                "--features",
                "rdkit-reference",
                "--test",
                "native_aromatic",
                "serialize_aromatic_requests",
                "--",
                "--ignored",
                "--exact",
                "--nocapture",
            ],
            cwd=ROOT,
            env=environment,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            encoding="utf-8",
        )
        sentinel = re.findall(
            r"Serialized (\d+) complete aromatic requests; \d+ preflight rejections; no worker expectations generated",
            completed.stdout,
        )
        if sentinel != [str(count)] or "test result: ok. 1 passed;" not in completed.stdout:
            raise RuntimeError(
                "Request serializer did not execute exactly once: " + completed.stdout
            )
        with gzip.open(fresh, "rt", encoding="utf-8") as stream:
            normalized = json.loads(next(stream))
            actual = sum(1 for _ in stream)
        if actual != count or normalized.get("request_encoding") != "reshiki-serde-request-v1":
            raise ValueError("Request serialization output is incomplete")
        fresh.replace(output)
    print(completed.stdout, end="")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--requests", type=Path)
    mode.add_argument("--corpus", action="store_true")
    mode.add_argument("--serialize", type=Path)
    args = parser.parse_args()
    if args.serialize:
        serialize_requests(args.serialize, args.output)
        return
    if rdBase.rdkitVersion != VERSION:
        raise ValueError(f"Expected RDKit {VERSION}, got {rdBase.rdkitVersion}")
    RDLogger.DisableLog("rdApp.*")
    metadata = provenance()
    if args.requests:
        with gzip.open(args.requests, "rt", encoding="utf-8") as stream:
            request_header = json.loads(next(stream))
        if request_header.get("request_encoding") != "reshiki-serde-request-v1":
            raise ValueError("Run the explicit Rust request-serialization test first")
        metadata["request_encoding"] = request_header["request_encoding"]
        metadata["request_source"] = request_header
        metadata["request_file_sha256"] = digest(args.requests)
    else:
        metadata["request_encoding"] = "raw-independent-corpus-v1"
    accepted = rejected = count = preflight = worker_accepted = worker_rejected = 0
    content_hash = hashlib.sha256()
    with tempfile.TemporaryFile(mode="w+t", encoding="utf-8", newline="\n") as records:
        for name, request, preflight_error in requests(args.requests):
            record = dict(name=name, request=request)
            if args.requests:
                record["preflight_error"] = preflight_error
                try:
                    record["expected"] = dict(ok=True, result=handle(copy.deepcopy(request)))
                    worker_accepted += 1
                except Exception as error:  # Match engine.worker.main's exact envelope.
                    record["expected"] = dict(ok=False, error=str(error))
                    worker_rejected += 1
                if preflight_error is not None:
                    preflight += 1
                if preflight_error is None and record["expected"]["ok"]:
                    accepted += 1
                else:
                    rejected += 1
            count += 1
            line = encoded(record) + "\n"
            content_hash.update(line.encode())
            records.write(line)
        if count <= 5500 or args.requests and (accepted <= 5000 or rejected <= 500):
            raise ValueError(f"Insufficient corpus: {accepted} accepted, {rejected} rejected")
        if args.requests and count != request_header["cases"]:
            raise ValueError("Serialized request count differs; input generation must run fully")
        metadata.update(
            cases=count,
            preflight_rejections=preflight,
            worker_accepted=worker_accepted,
            worker_rejected=worker_rejected,
            accepted=accepted,
            rejected=rejected,
            records_sha256=content_hash.hexdigest(),
        )
        args.output.parent.mkdir(parents=True, exist_ok=True)
        temporary = args.output.with_suffix(args.output.suffix + ".tmp")
        try:
            with (
                temporary.open("wb") as raw,
                gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0) as compressed,
                io.TextIOWrapper(compressed, encoding="utf-8", newline="\n") as stream,
            ):
                stream.write(encoded(metadata) + "\n")
                records.seek(0)
                for line in records:
                    stream.write(line)
            temporary.replace(args.output)
        finally:
            temporary.unlink(missing_ok=True)
    print(
        encoded(
            dict(
                output=str(args.output),
                accepted=accepted,
                rejected=rejected,
                bytes=args.output.stat().st_size,
                sha256=digest(args.output),
            )
        )
    )


if __name__ == "__main__":
    main()
