"""Verify the preserved Windows two-contact graph and compact producer records."""

import copy
import hashlib
import json
import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parent
HEAD = "44d198c4e6791a15017af7ac6d356c245b7073ca"
EXE = "752743a45e6afda49de4661d68bfcf07b7ef736c4aa3a3829d799171dae6a894"
BASE_EXE = "135779960a15c17bac51e00c614600422531a30b90480d6cb1ba8de5b795e51d"
INPUT_SHA = "0f6f575a908de66b33e0117386c66b288f8bbcb6348b543a50cde96e548f42d4"
OUTPUT_SHA = "d97663421707af458b258e1d37dcc0a5ab1b3ebcfb830d973ef87f43fefe4d56"
CARBONS = {3, 4, 7, 8, 11, 12}
RUN = 38002254770
JOB = 114062843997


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def read(name):
    return json.loads((ROOT / name).read_text(encoding="utf-8-sig"))


def sha(data):
    return hashlib.sha256(data).hexdigest()


def validate_native(before, after):
    require(before["version"] == after["version"] == 19, "native schema version")
    require(len(before["atoms"]) == len(after["atoms"]) == 13, "atom count")
    require(len(before["bonds"]) == 9 and len(after["bonds"]) == 11, "bond count")
    require(
        {a["id"] for a in before["atoms"] if a["element"] == "C"} == CARBONS,
        "original carbon identity",
    )
    expected = copy.deepcopy(before)
    for atom in expected["atoms"]:
        if atom["id"] in CARBONS:
            require(atom["label_h"] == 0, "original carbon cache")
            atom["label_h"] = 2
    for donor in [5, 2]:
        expected["bonds"].append(
            {
                "z_order": 0,
                "a": donor,
                "b": 1,
                "order": 5,
                "display": "plain",
                "stereo": None,
                "stereo_atoms": [],
                "double_position": "auto",
                "color": [0, 0, 0],
            }
        )
    require(
        after == expected,
        "native JSON differs beyond two directed contacts and six derived carbon caches",
    )


def validate_build(build, audit):
    require(
        build["qualified"]
        and build["pr"] == 283
        and build["repository"] == "Ameyanagi/ReShiki"
        and build["head_sha"] == audit["head"] == HEAD
        and int(build["run_id"]) == audit["run"] == RUN
        and int(build["attempt"]) == 1
        and audit["job"] == JOB,
        "exact head/run/job binding",
    )
    require(
        build["exe"]["sha256"] == audit["exe_sha256"] == EXE
        and build["exe"]["bytes"] == 55363584
        and build["git_tree"] == "ec53264e759c2f5353256882da4b24a6fdb6dac1",
        "executable/tree binding",
    )
    require(
        build["profile"] == "release"
        and build["target"] == "x86_64-pc-windows-msvc"
        and build["optional_features"] == []
        and build["signed"] is False
        and build["source_bytes_unchanged"]
        and build["tracked_source_count"] == audit["exact_Git_source_files"] == 2642,
        "qualified default release/source mode",
    )
    require(
        len(build["own_compiler_artifacts"]) == audit["own_fresh_compiler_artifacts"] == 15
        and all(a["fresh"] is False for a in build["own_compiler_artifacts"])
        and audit["actual_depfile_source_paths"] == 564
        and audit["result"] == "PASS"
        and audit["all_archive_and_extracted_bytes_match"],
        "recorded independent compiler/source audit",
    )


def validate_processes(baseline, candidate, reopened):
    for snapshot, pid, digest, suffix in [
        (baseline, 6988, BASE_EXE, r"artifacts\base-51fa0991\reshiki.exe"),
        (candidate, 12908, EXE, r"reshiki-pr283-ctrl-j-20261010\reshiki.exe"),
        (reopened, 8980, EXE, r"reshiki-pr283-ctrl-j-20261010\reshiki.exe"),
    ]:
        require(len(snapshot["processes"]) == 1, "one expected live editor")
        process = snapshot["processes"][0]
        require(
            process["pid"] == pid
            and process["session_id"] == 2
            and process["executable_sha256"] == digest
            and process["executable_path"].lower().endswith(suffix.lower())
            and process["creation_utc"] <= snapshot["observed_utc"],
            "live PID/session/executable binding",
        )
    require(
        candidate["original_fixture"]["bytes"] == 6722
        and candidate["original_fixture"]["sha256"] == INPUT_SHA,
        "original input still unchanged at initial candidate proof",
    )
    files = {Path(f["path"].replace("\\", "/")).name: f for f in reopened["owned_files"]}
    require(
        set(files) == {"co-en3-before.rsk", "co-en-windows-chelate-after.rsk"}, "owned native files"
    )
    require(
        files["co-en3-before.rsk"]["bytes"] == 6722
        and files["co-en3-before.rsk"]["sha256"] == INPUT_SHA
        and files["co-en-windows-chelate-after.rsk"]["bytes"] == 7208
        and files["co-en-windows-chelate-after.rsk"]["sha256"] == OUTPUT_SHA,
        "input and saved bytes still unchanged on fresh reopen",
    )


def jpeg_size(data):
    require(data[:2] == b"\xff\xd8", "JPEG signature")
    offset = 2
    while offset < len(data):
        require(data[offset] == 0xFF, "JPEG marker")
        while data[offset] == 0xFF:
            offset += 1
        marker = data[offset]
        offset += 1
        require(marker not in [0xD9, 0xDA], "JPEG dimensions missing")
        size = struct.unpack_from(">H", data, offset)[0]
        require(size >= 2 and offset + size <= len(data), "JPEG segment size")
        if marker in {0xC0, 0xC1, 0xC2}:
            height, width = struct.unpack_from(">HH", data, offset + 3)
            return [width, height]
        offset += size
    raise ValueError("JPEG dimensions missing")


def main():
    manifest = read("manifest.json")
    require(manifest["schema"] == 1, "manifest schema")
    require(
        set(manifest["files"])
        == {p.name for p in ROOT.iterdir() if p.is_file()} - {"manifest.json"},
        "complete package file inventory",
    )
    photos = 0
    for name, proof in manifest["files"].items():
        require(Path(name).name == name, "local evidence path")
        data = (ROOT / name).read_bytes()
        require(
            len(data) == proof["bytes"] and sha(data) == proof["sha256"],
            "evidence byte identity: " + name,
        )
        if name.endswith(".jpg"):
            require(jpeg_size(data) == [2556, 1712], "untouched capture dimensions")
            photos += 1
    require(photos == 8, "eight approved native screenshots")
    require(sha((ROOT / "co-en3-before.rsk").read_bytes()) == INPUT_SHA, "original input hash")
    require(
        sha((ROOT / "co-en-windows-chelate-after.rsk").read_bytes()) == OUTPUT_SHA,
        "native output hash",
    )
    before, after = read("co-en3-before.rsk"), read("co-en-windows-chelate-after.rsk")
    build, audit = read("windows-build-receipt.txt"), read("root-artifact-source-audit.txt")
    baseline, candidate, reopened = [
        read(n)
        for n in [
            "baseline-live-process.txt",
            "candidate-live-process.txt",
            "fresh-reopen-process.txt",
        ]
    ]
    validate_native(before, after)
    validate_build(build, audit)
    validate_processes(baseline, candidate, reopened)
    controls = []
    mutations = [
        (
            "N10 coordinate move",
            lambda d: d["atoms"][9]["position"].__setitem__(
                "x", d["atoms"][9]["position"]["x"] + 1
            ),
        ),
        ("Co charge change", lambda d: d["atoms"][0].__setitem__("charge", 2)),
        ("isotope change", lambda d: d["atoms"][1].__setitem__("isotope", 15)),
        ("donor flag change", lambda d: d["atoms"][1].__setitem__("no_implicit", True)),
        ("organic bond change", lambda d: d["bonds"][0].__setitem__("order", 2)),
        ("reversed coordination direction", lambda d: d["bonds"][9].update(a=1, b=5)),
        ("extra third contact", lambda d: d["bonds"].append(dict(d["bonds"][9], a=6))),
        ("NH2 label change", lambda d: d["atoms"][1].__setitem__("label_h", 1)),
    ]
    for name, mutate in mutations:
        bad = copy.deepcopy(after)
        mutate(bad)
        try:
            validate_native(before, bad)
        except ValueError:
            controls.append(name)
        else:
            raise AssertionError("Control accepted: " + name)
    for name, mutate in [
        ("stale source head", lambda d: d.__setitem__("head_sha", "0" * 40)),
        ("wrong candidate executable", lambda d: d["exe"].__setitem__("sha256", "0" * 64)),
    ]:
        bad = copy.deepcopy(build)
        mutate(bad)
        try:
            validate_build(bad, audit)
        except ValueError:
            controls.append(name)
        else:
            raise AssertionError("Control accepted: " + name)
    print(
        json.dumps(
            {
                "result": "PASS",
                "exact_files": len(manifest["files"]),
                "raw_JPEGs": photos,
                "atoms": 13,
                "bonds": 11,
                "negative_controls_rejected": controls,
                "GUI_replayed": False,
            }
        )
    )


if __name__ == "__main__":
    main()
