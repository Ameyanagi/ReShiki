"""Audit/recreate the checked-in offline RDKit geometry source archives.

Normal builds only consume these archives. Refreshing explicitly requires the
pinned upstream checkout and Boost include tree; this script never downloads.
"""

import argparse
import gzip
import hashlib
import io
import json
import re
import subprocess
import tarfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
VENDOR = Path("native/geometry/vendor")
RDKIT_VERSION = "2026.03.6"
RDKIT_REVISION = "0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985"
BOOST_VERSION = "1.92.0"
BOOST_UPSTREAM_ARCHIVE = {
    "url": "https://github.com/boostorg/boost/releases/download/boost-1.92.0/boost-1.92.0-b2-nodocs.tar.xz",
    "sha256": "ea7b982002cc9dfbe59b0b217b206f470dc75f3de0bb2973d844118934d82411",
}
LIBRARIES = frozenset(
    """DistGeomHelpers MolAlign ForceFieldHelpers SubstructMatch
GraphMol DistGeometry Alignment MolTransforms SmilesParse GenericGroups
RDGeometryLib DataStructs RDGeneral EigenSolvers ForceField Optimizer Trajectory""".split()
)
GENERATED_PARSERS = (
    "lex.yysmiles.cpp",
    "lex.yysmarts.cpp",
    "smiles.tab.cpp",
    "smiles.tab.hpp",
    "smarts.tab.cpp",
    "smarts.tab.hpp",
)
LITERAL_INCLUDE = re.compile(r'^\s*#\s*include\s*[<"]([^>"]+)[>"]', re.MULTILINE)
MACRO_INCLUDE = re.compile(r'^\s*#\s*include\s+([^<"\s].*)', re.MULTILINE)


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def metadata(root=ROOT):
    vendor = Path(root) / VENDOR
    manifest = json.loads((vendor / "source-manifest.json").read_text())
    return {
        "version": manifest["rdkit"]["version"],
        "revision": manifest["rdkit"]["revision"],
        "boost_version": manifest["boost"]["version"],
        "manifest_sha256": digest(vendor / "source-manifest.json"),
        "archives": {name: record["sha256"] for name, record in manifest["archives"].items()},
        "runtime": "self-process-static-core",
    }


def verify(root=ROOT):
    vendor = Path(root) / VENDOR
    manifest = json.loads((vendor / "source-manifest.json").read_text())
    if (
        manifest["rdkit"]["version"] != RDKIT_VERSION
        or manifest["rdkit"]["revision"] != RDKIT_REVISION
    ):
        raise ValueError("Geometry source does not match the pinned RDKit release")
    if manifest["boost"]["version"] != BOOST_VERSION:
        raise ValueError("Geometry source does not match the pinned Boost headers")
    for name, record in manifest["archives"].items():
        archive = vendor / name
        if archive.stat().st_size != record["bytes"] or digest(archive) != record["sha256"]:
            raise ValueError(f"Geometry source archive changed: {name}")
        found = {}
        with tarfile.open(archive, "r:gz") as stream:
            for member in stream:
                path = Path(member.name)
                if not member.isfile() or path.is_absolute() or ".." in path.parts:
                    raise ValueError(f"Unsafe geometry archive member: {member.name}")
                if member.name in found or member.name not in record["files"]:
                    raise ValueError(f"Unexpected geometry archive member: {member.name}")
                content = stream.extractfile(member)
                if content is None:
                    raise ValueError(f"Missing geometry archive contents: {member.name}")
                with content:
                    data = content.read()
                found[member.name] = hashlib.sha256(data).hexdigest()
        if found != record["files"]:
            raise ValueError(f"Geometry source contents changed: {name}")
    return metadata(root)


def library_sources(code):
    libraries = {}
    exports = set()
    for cmake in sorted(code.rglob("CMakeLists.txt")):
        for body in re.findall(r"rdkit_library\s*\(([^)]+)\)", cmake.read_text()):
            words = body.split()
            exports.add(words[0])
            if words[0] not in LIBRARIES:
                continue
            stop = words.index("LINK_LIBRARIES") if "LINK_LIBRARIES" in words else len(words)
            libraries[words[0]] = [
                (cmake.parent / word).relative_to(code).as_posix()
                for word in words[1:stop]
                if word.endswith(".cpp") and word != "SmilesJSONParsers.cpp"
            ]
    if set(libraries) != LIBRARIES:
        raise ValueError("Pinned RDKit geometry library closure changed")
    return libraries, sorted(exports)


def include_closure(code, boost, roots):
    """Preserve literal conditional includes and macro-owning Boost modules.

    Expanding a module for computed #include names keeps its platform/compiler
    alternatives, instead of capturing only headers used by the current host.
    Missing legacy/optional includes are recorded, never silently synthesized.
    """
    # These libraries compute filenames in #define expansion, including local
    # and forward iteration headers that never appear as literal #include lines.
    computed_modules = {"preprocessor", "mpl"}
    pending = list(roots)
    pending.extend(
        p for module in computed_modules for p in (boost / module).rglob("*") if p.is_file()
    )
    seen, expanded, missing = set(), computed_modules.copy(), set()
    while pending:
        path = pending.pop()
        if path in seen:
            continue
        seen.add(path)
        source = path.read_text(errors="strict")
        if path.is_relative_to(boost) and MACRO_INCLUDE.search(source):
            module = path.relative_to(boost).parts[0]
            directory = boost / module
            if directory.is_dir() and module not in expanded:
                expanded.add(module)
                pending.extend(p for p in directory.rglob("*") if p.is_file())
        for include in LITERAL_INCLUDE.findall(source):
            if include.startswith("boost/"):
                candidate = boost / include[6:]
                if candidate.is_file():
                    pending.append(candidate)
                else:
                    missing.add(include)
                continue
            for candidate in (path.parent / include, code / include):
                candidate = candidate.resolve()
                if not candidate.is_relative_to(code) and not candidate.is_relative_to(boost):
                    continue
                if candidate.is_file():
                    pending.append(candidate)
                    break
                template = candidate.with_name(candidate.name + ".cmake")
                if template.is_file():
                    pending.append(template)
                    break
    return seen, sorted(expanded), sorted(missing)


def archive_sources(destination, paths):
    hashes, uncompressed = {}, 0
    with destination.open("wb") as raw:
        with gzip.GzipFile(
            filename="", mode="wb", fileobj=raw, compresslevel=9, mtime=0
        ) as compressed:
            with tarfile.open(fileobj=compressed, mode="w", format=tarfile.USTAR_FORMAT) as stream:
                for name, source in sorted(paths.items()):
                    content = source.read_bytes()
                    info = tarfile.TarInfo(name)
                    info.size, info.mode = len(content), 0o644
                    info.mtime, info.uid, info.gid = 0, 0, 0
                    stream.addfile(info, io.BytesIO(content))
                    hashes[name] = hashlib.sha256(content).hexdigest()
                    uncompressed += len(content)
    return {
        "sha256": digest(destination),
        "bytes": destination.stat().st_size,
        "source_bytes": uncompressed,
        "files": hashes,
    }


def refresh(rdkit, boost_include, root=ROOT):
    rdkit, boost_include = Path(rdkit).resolve(), Path(boost_include).resolve()
    revision = subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=rdkit, check=True, capture_output=True, text=True
    ).stdout.strip()
    if revision != RDKIT_REVISION:
        raise ValueError("Supply the exact pinned upstream RDKit checkout")
    subprocess.run(["git", "diff", "--exit-code", "HEAD", "--", "Code"], cwd=rdkit, check=True)
    code, boost = rdkit / "Code", boost_include / "boost"
    if "#define BOOST_VERSION 109200" not in (boost / "version.hpp").read_text():
        raise ValueError("Supply Boost 1.92.0 headers")
    libraries, exports = library_sources(code)
    roots = []
    for sources in libraries.values():
        roots.extend(
            code / (name + ".cmake" if name == "RDGeneral/versions.cpp" else name)
            for name in sources
        )
    roots.extend(code / "GraphMol/SmilesParse" / (name + ".cmake") for name in GENERATED_PARSERS)
    roots.extend(code / "RDGeneral" / name for name in ("versions.h.cmake", "RDConfig.h.cmake"))
    roots.extend(
        code / "GraphMol/ForceFieldHelpers" / name
        for name in ("FFConvenience.h", "UFF/UFF.h", "MMFF/MMFF.h")
    )
    paths, modules, missing = include_closure(code, boost, roots)
    vendor = Path(root) / VENDOR
    vendor.mkdir(parents=True, exist_ok=True)
    archives = {}
    archives["rdkit-geometry-2026.03.6.tar.gz"] = archive_sources(
        vendor / "rdkit-geometry-2026.03.6.tar.gz",
        {
            "rdkit/Code/" + p.relative_to(code).as_posix(): p
            for p in paths
            if p.is_relative_to(code)
        },
    )
    archives["boost-geometry-1.92.0.tar.gz"] = archive_sources(
        vendor / "boost-geometry-1.92.0.tar.gz",
        {
            "boost/boost/" + p.relative_to(boost).as_posix(): p
            for p in paths
            if p.is_relative_to(boost)
        },
    )
    manifest = {
        "format": 1,
        "rdkit": {
            "version": RDKIT_VERSION,
            "revision": RDKIT_REVISION,
            "repository": "https://github.com/rdkit/rdkit",
            "libraries": libraries,
            "export_macros": exports,
            "omitted": ["GraphMol/SmilesParse/SmilesJSONParsers.cpp"],
        },
        "boost": {
            "version": BOOST_VERSION,
            "upstream_archive": BOOST_UPSTREAM_ARCHIVE,
            "capture": "Installed unmodified Homebrew Boost headers; individually SHA-256 pinned",
            "expanded_macro_include_modules": modules,
            "unavailable_optional_or_legacy_includes": missing,
        },
        "archives": archives,
    }
    (vendor / "source-manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n"
    )
    hashes = "\n".join(
        f'set({"RDKIT" if name.startswith("rdkit") else "BOOST"}_SOURCE_SHA256 "{record["sha256"]}")'
        for name, record in archives.items()
    )
    (vendor / "source-hashes.cmake").write_text(hashes + "\n")
    sources = [name for group in libraries.values() for name in group]
    sources.extend(
        "GraphMol/SmilesParse/" + name for name in GENERATED_PARSERS if name.endswith(".cpp")
    )
    lines = [
        "# Sources and export names from the exact pinned upstream CMake library closure.",
        "set(RDKIT_GEOMETRY_SOURCES",
    ]
    for name in sorted(sources):
        base = (
            "GENERATED_CODE"
            if name == "RDGeneral/versions.cpp" or name.split("/")[-1] in GENERATED_PARSERS
            else "RDKIT_CODE"
        )
        lines.append(f'  "${{{base}}}/{name}"')
    lines.extend([")", "set(RDKIT_EXPORT_NAMES " + " ".join(exports) + ")", ""])
    (Path(root) / "native/geometry/cpp/sources.cmake").write_text("\n".join(lines))
    return verify(root)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--refresh", action="store_true")
    parser.add_argument("--rdkit-source", type=Path)
    parser.add_argument("--boost-include", type=Path)
    args = parser.parse_args()
    if args.refresh:
        if not args.rdkit_source or not args.boost_include:
            parser.error("--refresh requires --rdkit-source and --boost-include")
        result = refresh(args.rdkit_source, args.boost_include)
    else:
        if args.rdkit_source or args.boost_include:
            parser.error("Source paths only apply to --refresh")
        result = verify()
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
