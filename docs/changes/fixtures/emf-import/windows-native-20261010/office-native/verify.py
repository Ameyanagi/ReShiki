#!/usr/bin/env python3
"""Check retained Office containers and process receipts without Office or the app."""

import copy
import hashlib
import json
import posixpath
import struct
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[5]
NS = {
    "p": "http://schemas.openxmlformats.org/presentationml/2006/main",
    "a": "http://schemas.openxmlformats.org/drawingml/2006/main",
    "r": "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
    "wp": "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing",
    "pic": "http://schemas.openxmlformats.org/drawingml/2006/picture",
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def picture(parts, kind, expected_emf):
    if kind == "powerpoint":
        document, prefix, media_prefix = "ppt/slides/slide1.xml", "p", "ppt/media/"
        slides = [n for n in parts if n.startswith("ppt/slides/slide") and n.endswith(".xml")]
        require(slides == [document], "one slide")
    else:
        document, prefix, media_prefix = "word/document.xml", "pic", "word/media/"
    root = ET.fromstring(parts[document])
    pics = root.findall(f".//{prefix}:pic", NS)
    require(len(pics) == 1, "one picture")
    pic = pics[0]
    if kind == "powerpoint":
        require(root.find("p:cSld/p:spTree/p:pic", NS) is pic, "ungrouped slide picture")
        require(not root.findall(".//p:grpSp", NS), "no group transform")
    else:
        inlines = root.findall(".//wp:inline", NS)
        require(len(inlines) == 1 and not root.findall(".//wp:anchor", NS), "one inline picture")
        require(
            inlines[0].find("a:graphic/a:graphicData/pic:pic", NS) is pic, "direct inline picture"
        )
    fill = pic.find(f"{prefix}:blipFill", NS)
    require(fill.find("a:srcRect", NS) is None, "no crop")
    require(fill.find("a:stretch/a:fillRect", NS).attrib == {}, "untrimmed fill rectangle")
    rid = fill.find("a:blip", NS).get("{" + NS["r"] + "}embed")
    relpath = posixpath.join(
        posixpath.dirname(document), "_rels", posixpath.basename(document) + ".rels"
    )
    relations = [r for r in ET.fromstring(parts[relpath]) if r.get("Id") == rid]
    require(len(relations) == 1, "unique picture relationship")
    relation = relations[0]
    require(relation.get("TargetMode") != "External", "embedded media")
    require(relation.get("Type", "").endswith("/image"), "image relationship")
    media = posixpath.normpath(posixpath.join(posixpath.dirname(document), relation.get("Target")))
    require(
        [n for n in parts if n.startswith(media_prefix)] == [media], "one referenced media item"
    )
    require(parts[media] == expected_emf, "byte-exact desktop EMF")
    transform = pic.find(f"{prefix}:spPr/a:xfrm", NS)
    require(
        all(transform.get(key, "0") in ("0", "false") for key in ("rot", "flipH", "flipV")),
        "no rotation or flip",
    )
    extent = transform.find("a:ext", NS)
    size = [int(extent.get(k)) for k in ("cx", "cy")]
    require(size == [3701880, 2261880], "recorded Office extent")
    if kind == "word":
        inline_extent = inlines[0].find("wp:extent", NS)
        require(
            [int(inline_extent.get(k)) for k in ("cx", "cy")] == size,
            "inline extent matches picture",
        )
        effects = {k: int(v) for k, v in inlines[0].find("wp:effectExtent", NS).attrib.items()}
        require(effects == {"l": 0, "t": 0, "r": 0, "b": 5080}, "separate Word effect extent")
    frame = struct.unpack_from("<4i", expected_emf, 24)
    frame_emu = [(frame[2] - frame[0]) * 360, (frame[3] - frame[1]) * 360]
    require(
        [a - b for a, b in zip(size, frame_emu)] == [360, 360],
        "explicit 0.01 mm per-axis difference",
    )
    return {
        "media": media,
        "embedded_emf_sha256": sha(parts[media]),
        "picture_extent_emu": size,
        "picture_mm": [n / 36000 for n in size],
        "frame_mm": [n / 36000 for n in frame_emu],
        "difference_mm": [0.01, 0.01],
        "crop_rotation_flip": False,
    }


def jpeg_size(data):
    require(data[:2] == b"\xff\xd8", "JPEG signature")
    cursor = 2
    while cursor < len(data):
        require(data[cursor] == 255, "JPEG marker")
        while data[cursor] == 255:
            cursor += 1
        marker = data[cursor]
        cursor += 1
        length = struct.unpack_from(">H", data, cursor)[0]
        require(length >= 2 and cursor + length <= len(data), "JPEG segment")
        if marker in (192, 193, 194):
            height, width = struct.unpack_from(">HH", data, cursor + 3)
            return width, height
        cursor += length
    raise ValueError("missing JPEG frame")


def controls(parts, kind, expected_emf):
    document = "ppt/slides/slide1.xml" if kind == "powerpoint" else "word/document.xml"
    prefix = "p" if kind == "powerpoint" else "pic"
    rejected = []
    for label in ("one-EMU extent change", "crop", "rotation", "altered embedded EMF"):
        changed = copy.copy(parts)
        root = ET.fromstring(parts[document])
        pic = root.find(f".//{prefix}:pic", NS)
        if label == "one-EMU extent change":
            pic.find(f"{prefix}:spPr/a:xfrm/a:ext", NS).set("cx", "3701881")
        elif label == "crop":
            ET.SubElement(
                pic.find(f"{prefix}:blipFill", NS), "{" + NS["a"] + "}srcRect", {"l": "1"}
            )
        elif label == "rotation":
            pic.find(f"{prefix}:spPr/a:xfrm", NS).set("rot", "60000")
        else:
            media = next(n for n in parts if n.endswith("/image1.emf"))
            changed[media] = changed[media][:-1] + bytes([changed[media][-1] ^ 1])
        changed[document] = ET.tostring(root)
        try:
            picture(changed, kind, expected_emf)
        except ValueError:
            rejected.append(kind + ": " + label)
        else:
            raise ValueError("accepted negative control: " + label)
    return rejected


def main():
    manifest = json.loads((HERE / "evidence-manifest.json").read_bytes())
    screenshots = 0
    for entry in manifest["files"]:
        path = (REPO / entry["path"]).resolve()
        require(REPO in path.parents, "manifest path inside repository")
        data = path.read_bytes()
        require(len(data) == entry["bytes"] and sha(data) == entry["sha256"], "copied bytes/hash")
        if path.suffix == ".jpg":
            require(jpeg_size(data) == (2556, 1712), "raw screenshot dimensions")
            screenshots += 1
    require(screenshots == 4, "four selected save/reopen captures")
    expected_emf = (HERE.parent / "desktop-vector-reexport-20261010.emf").read_bytes()
    require(
        sha(expected_emf) == "a1c18d84970b68b064a6e7a8866cfb43e0b0922001e476a4e9f4de004c170c1a",
        "source export identity",
    )
    result, rejected = {}, []
    for kind, suffix in (("powerpoint", "pptx"), ("word", "docx")):
        path = HERE / f"desktop-{kind}-emf-20261010.{suffix}"
        with zipfile.ZipFile(path) as archive:
            require(archive.testzip() is None, "ZIP integrity")
            require(len(archive.namelist()) == len(set(archive.namelist())), "unique ZIP entries")
            parts = {name: archive.read(name) for name in archive.namelist()}
        result[kind] = picture(parts, kind, expected_emf)
        rejected.extend(controls(parts, kind, expected_emf))
        initial = json.loads((HERE / f"{kind}-readonly-ssh-save-audit.json").read_bytes())
        reopened = json.loads((HERE / f"{kind}-fresh-reopen-process-audit.json").read_bytes())
        require(initial["first_read"] == initial["second_read"], "stable initial reads")
        fresh_file = reopened["owned_file"] if kind == "powerpoint" else reopened["first_read"]
        for key in ("bytes", "sha256", "modified_utc"):
            require(initial["first_read"][key] == fresh_file[key], "unchanged fresh-reopen file")
        require(
            fresh_file["sha256"] == sha(path.read_bytes()), "container/reopen receipt correlation"
        )
        require(reopened["owned_file_unchanged_since_save"], "recorded unchanged file")
        process = reopened[kind + "_processes"][0]
        require(
            process["session_id"] == 2 and process["file_version"] == "16.0.20527.20032",
            "recorded Office process",
        )
        if kind == "word":
            require(
                not reopened["old_pid_5024_exists"] and process["pid"] != 5024, "fresh Word PID"
            )
            require(reopened["first_read"] == reopened["second_read"], "stable reopened Word reads")
        else:
            require(not initial["powerpoint_processes"], "prior PowerPoint exit observation")
            require(
                process["created_utc"] > initial["observed_utc"],
                "PowerPoint started after absent snapshot",
            )
        screenshot = reopened["root_reopen_screenshot"]
        matching = [
            e for e in manifest["files"] if e["path"].endswith(Path(screenshot["path"]).name)
        ]
        require(
            len(matching) == 1 and matching[0]["sha256"] == screenshot["sha256"],
            "reopen screenshot correlation",
        )
        result[kind]["fresh_pid"] = process["pid"]
        result[kind]["file_unchanged_after_reopen"] = True
    root_receipt = json.loads((HERE / "root-office-native-acceptance.json").read_bytes())
    require(root_receipt["status"] == "PASS", "root acceptance status")
    require(
        root_receipt["core_native_receipt_sha256"]
        == sha((HERE.parent / "native-verification.json").read_bytes()),
        "root/core receipt correlation",
    )
    for label, kind in (("Word", "word"), ("PowerPoint", "powerpoint")):
        claim = root_receipt["office"][label]
        raw = (HERE / claim["file"]).read_bytes()
        require(
            len(raw) == claim["bytes"] and sha(raw) == claim["sha256"], "root/container identity"
        )
        require(
            claim["picture_extent_emu"] == result[kind]["picture_extent_emu"],
            "root/exact picture extent",
        )
        require(claim["picture_extent_mm"] == result[kind]["picture_mm"], "root/mm extent")
        require(
            claim["source_frame_mm"] == result[kind]["frame_mm"]
            and claim["extent_delta_mm"] == [0.01, 0.01],
            "root/frame difference",
        )
        require(
            claim["embedded_emf_exact"] and claim["no_crop_rotation_flip"], "root/picture semantics"
        )
        base = "word" if kind == "word" else "ppt/slides"
        require(
            posixpath.normpath(posixpath.join(base, claim["relationship_target"]))
            == result[kind]["media"],
            "root/exact referenced picture relationship",
        )
    by_name = {Path(entry["path"]).name: entry for entry in manifest["files"]}
    for group in ("selected_original_screenshots", "independent_readonly_receipts"):
        for name, expected in root_receipt[group].items():
            require(
                all(by_name[name][key] == expected[key] for key in ("bytes", "sha256")),
                "root/evidence hash correlation",
            )
    print(
        json.dumps(
            {
                "status": "PASS",
                "root_acceptance_receipt_correlated": True,
                "raw_copies": len(manifest["files"]),
                "raw_bytes": sum(e["bytes"] for e in manifest["files"]),
                "screenshots": screenshots,
                "containers": result,
                "negative_controls_rejected": rejected,
                "scope": "Retained Office file/process/screenshot correlation, not new GUI execution or a universal Office compatibility test.",
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
