"""Build live depiction observers for the optional Linux RDKit reference tests.

Run only after installing the pinned reference wheel. Never cache observer
binaries: their native answers belong to this CPU, libc and installed wheel.
Downloaded Boost and RingDecomposerLib archives are reusable only after their
hashes are verified.
"""

import argparse
import hashlib
import json
import os
import platform
import re
import shlex
import shutil
import subprocess
import sys
import sysconfig
import tarfile
import tempfile
import urllib.request
from pathlib import Path

from download_tls import verified_context

PIN = "fece8caa860bdf6c9c82bdb9253f22a0cf87150c"
BOOST_SHA256 = "7009fe1faa1697476bdc7027703a2badb84e849b7b0baad5086b087b971f8617"
BOOST_URL = "https://archives.boost.io/release/1.85.0/source/boost_1_85_0.tar.bz2"
# RDKit's External/RingFamilies CMake pins this archive by MD5; RingInfo.h includes its header.
RDL_MD5 = "e9a0bcdda8b921a35e812b9888a9a874"
RDL_URL = "https://github.com/rareylab/RingDecomposerLib/archive/v1.1.3_rdkit.tar.gz"
RDL_HEADER = "RingDecomposerLib-1.1.3_rdkit/src/RingDecomposerLib/RingDecomposerLib.h"
ROOT = Path(__file__).resolve().parents[1]
COMPONENTS = (
    "geometry",
    "rings",
    "attachment",
    "seeds",
    "templates",
    "collision",
    "expansion",
    "finalize",
)


def digest(path, algorithm="sha256"):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, algorithm).hexdigest()


def checked_source(path):
    head = subprocess.check_output(["git", "-C", str(path), "rev-parse", "HEAD"], text=True).strip()
    if head != PIN:
        raise ValueError(f"Expected RDKit {PIN}, found {head}")
    subprocess.run(
        ["git", "-C", str(path), "diff", "--exit-code", "HEAD", "--", "Code"], check=True
    )
    extra = subprocess.check_output(
        ["git", "-C", str(path), "ls-files", "--others", "--", "Code"],
        text=True,
    )
    if extra:
        raise ValueError("The reference source contains untracked files under Code")
    return path.resolve()


def source_checkout(path, *, initialize=None):
    if not path.exists():
        path.parent.mkdir(parents=True, exist_ok=True)
        # Publish only after fetching and verifying the exact commit. Source
        # checkouts and their Git metadata are deliberately not CI cache input.
        with tempfile.TemporaryDirectory(prefix="rdkit-fetch-", dir=path.parent) as temporary:
            if initialize is None:
                subprocess.run(["git", "init", "--quiet", temporary], check=True)
            else:
                initialize(Path(temporary))
            subprocess.run(
                [
                    "git",
                    "-C",
                    temporary,
                    "fetch",
                    "--depth=1",
                    "https://github.com/rdkit/rdkit.git",
                    PIN,
                ],
                check=True,
                timeout=300,
            )
            subprocess.run(
                ["git", "-C", temporary, "checkout", "--quiet", "--detach", "FETCH_HEAD"],
                check=True,
            )
            checked_source(Path(temporary))
            Path(temporary).rename(path)
    return checked_source(path)


def cached_download(cache, url, name, algorithm, expected):
    cache.mkdir(parents=True, exist_ok=True)
    archive = cache / name
    if archive.exists():
        if digest(archive, algorithm) != expected:
            raise ValueError(f"Cached archive hash differs: {archive}")
        return archive
    with tempfile.TemporaryDirectory(prefix="download-", dir=cache) as temporary:
        download = Path(temporary) / "archive"
        size = 0
        with (
            urllib.request.urlopen(url, timeout=60, context=verified_context()) as response,
            download.open("wb") as output,
        ):
            while chunk := response.read(1024 * 1024):
                size += len(chunk)
                if size > 256 * 1024 * 1024:
                    raise ValueError(f"{name} exceeds the download bound")
                output.write(chunk)
        if digest(download, algorithm) != expected:
            raise ValueError(f"Downloaded {name} hash differs")
        download.rename(archive)
    return archive


def boost_archive(cache):
    name = f"boost-1.85.0-{BOOST_SHA256}.tar.bz2"
    return cached_download(cache, BOOST_URL, name, "sha256", BOOST_SHA256)


def ring_decomposer_header(source, cache):
    """Place RDKit's RingDecomposerLib header where its CMake build extracts it."""
    target = source / "External/RingFamilies/RingDecomposerLib/src/RingDecomposerLib"
    if not (target / "RingDecomposerLib.h").is_file():
        name = f"ringdecomposerlib-1.1.3_rdkit-{RDL_MD5}.tar.gz"
        archive = cached_download(cache, RDL_URL, name, "md5", RDL_MD5)
        target.mkdir(parents=True, exist_ok=True)
        with tarfile.open(archive, "r:gz") as source_archive:
            stream = source_archive.extractfile(RDL_HEADER)
            if stream is None:
                raise ValueError("Missing RingDecomposerLib header")
            with stream, (target / "RingDecomposerLib.h").open("wb") as output:
                shutil.copyfileobj(stream, output)


def extract_headers(archive, destination):
    # Extract only regular headers/directories from the verified release. Do not
    # restore executable scripts, links or archive-provided ownership metadata.
    total, count = 0, 0
    with tarfile.open(archive, "r:bz2") as source:
        for member in source:
            parts = Path(member.name).parts
            if parts[:2] != ("boost_1_85_0", "boost"):
                continue
            count += 1
            total += member.size
            if count > 50_000 or total > 300 * 1024 * 1024:
                raise ValueError("Boost headers exceed the extraction bound")
            if ".." in parts or not (member.isdir() or member.isfile()):
                raise ValueError(f"Unsupported Boost archive entry: {member.name}")
            target = destination.joinpath(*parts[1:])
            if member.isdir():
                target.mkdir(parents=True, exist_ok=True)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                stream = source.extractfile(member)
                if stream is None:
                    raise ValueError(f"Missing header contents: {member.name}")
                with stream, target.open("wb") as output:
                    shutil.copyfileobj(stream, output)
    header = (destination / "boost/version.hpp").read_text()
    if not re.search(r"#define BOOST_VERSION\s+108500\b", header):
        raise ValueError("Unexpected Boost version header")


def build(source, boost, directory, compiler, environment):
    # GNU ld may discard libpython used only by transitive Boost dependencies
    # with --as-needed, particularly on Ubuntu ARM. Keep this linker policy
    # local to observer builds; it never changes Rust/native helper compilation.
    wrapper = directory / "cxx"
    wrapper.write_text(f'#!/bin/sh\nexec {shlex.quote(compiler)} -Wl,--no-as-needed "$@"\n')
    wrapper.chmod(0o755)
    environment = {**environment, "CXX": str(wrapper)}
    common = ["--rdkit-source", str(source), "--boost-include", str(boost)]
    oracles = {}
    for component in COMPONENTS:
        if component in ("geometry", "rings", "attachment", "seeds"):
            builder = "build_depict_geometry_oracle.py"
            extra = [
                "--component",
                component,
                "--replay",
                "--output",
                str(directory / f"{component}.json.gz"),
            ]
        elif component == "finalize":
            builder = "build_depict_finalize_oracle.py"
            extra = ["--replay", "--output", str(directory / "finalize.json.gz")]
        else:
            builder = f"build_depict_{component}_oracle.py"
            extra = []
        command = [sys.executable, str(ROOT / "reference" / builder), *common, *extra]
        print(f"Building live {component} observer", flush=True)
        subprocess.run(command, check=True, cwd=ROOT, env=environment, timeout=600)
        binary = ROOT / "artifacts" / f"depict-{component}-oracle"
        if component == "finalize":
            binary /= "oracle"
        if not binary.is_file() or not os.access(binary, os.X_OK):
            raise ValueError(f"Observer was not built: {binary}")
        oracles[component] = binary.resolve()
    return oracles


def publish_environment(values, directory, github_env):
    if any("\n" in value or "\r" in value for value in values.values()):
        raise ValueError("Reference environment values must fit on one line")
    (directory / "environment.json").write_text(json.dumps(values, indent=2) + "\n")
    (directory / "environment.sh").write_text(
        "".join(f"export {key}={shlex.quote(value)}\n" for key, value in values.items())
    )
    if github_env:
        with github_env.open("a") as output:
            output.writelines(f"{key}={value}\n" for key, value in values.items())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--rdkit-source", type=Path, help="Use a clean pinned Git checkout instead of fetching"
    )
    parser.add_argument("--cache-dir", type=Path, default=ROOT / "artifacts/depict-download-cache")
    parser.add_argument("--github-env", type=Path, default=os.environ.get("GITHUB_ENV"))
    args = parser.parse_args()
    if sys.platform != "linux":
        raise SystemExit("This reference setup is Linux-only")
    import rdkit
    from rdkit import rdBase

    if (rdBase.rdkitVersion, rdBase.boostVersion) != ("2026.09.1", "1_85"):
        raise ValueError("Install the pinned RDKit 2026.09.1 wheel with Boost 1_85 first")
    compiler = shutil.which(os.environ.get("CXX", "c++"))
    if compiler is None:
        raise ValueError("A C++20 compiler executable is required")
    directory = ROOT / "artifacts/depict-live"
    directory.mkdir(parents=True, exist_ok=True)
    source = (
        checked_source(args.rdkit_source)
        if args.rdkit_source
        else source_checkout(directory / "rdkit")
    )
    archive = boost_archive(args.cache_dir.resolve())
    ring_decomposer_header(source, args.cache_dir.resolve())
    libraries = Path(rdkit.__file__).resolve().parent.parent / "rdkit.libs"
    python_library = Path(sysconfig.get_config_var("LIBDIR")) / sysconfig.get_config_var(
        "LDLIBRARY"
    )
    if not libraries.is_dir() or not python_library.is_file():
        raise ValueError("The installed wheel or Python shared library is missing")
    library_path = os.pathsep.join(
        filter(
            None,
            (str(libraries), str(python_library.parent), os.environ.get("LD_LIBRARY_PATH", "")),
        )
    )
    environment = {**os.environ, "LD_LIBRARY_PATH": library_path}
    # Header contents are re-extracted from the verified archive on every run;
    # neither extracted headers nor observer executables are trusted cache data.
    with tempfile.TemporaryDirectory(prefix="boost185-", dir=directory) as temporary:
        boost = Path(temporary)
        extract_headers(archive, boost)
        oracles = build(source, boost, directory, compiler, environment)
    values = {
        "RESHIKI_RDKIT_SOURCE": str(source),
        "DEPICT_EXPANSION_SOURCE": str(source),
        "LD_LIBRARY_PATH": library_path,
    }
    for component, binary in oracles.items():
        key = (
            "RESHIKI_DEPICT_ORACLE"
            if component == "geometry"
            else "DEPICT_EXPANSION_ORACLE"
            if component == "expansion"
            else f"RESHIKI_DEPICT_{component.upper()}_ORACLE"
        )
        values[key] = str(binary)
    metadata = dict(
        platform=platform.platform(),
        machine=platform.machine(),
        python=sys.version,
        rdkit=rdBase.rdkitVersion,
        boost=rdBase.boostVersion,
        source_commit=PIN,
        compiler=subprocess.check_output([compiler, "--version"], text=True),
        boost_archive_sha256=digest(archive),
        oracles={name: digest(path) for name, path in oracles.items()},
        libraries={
            path.name: digest(path) for path in sorted(libraries.glob("*.so*")) if path.is_file()
        },
        environment=values,
    )
    (directory / "provenance.json").write_text(json.dumps(metadata, indent=2) + "\n")
    publish_environment(values, directory, args.github_env)
    print(f"All eight live observers are ready; local environment: {directory / 'environment.sh'}")


if __name__ == "__main__":
    main()
