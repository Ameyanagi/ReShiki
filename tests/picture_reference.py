"""Independent Pillow reference corpus for raster migration."""

import base64
import io
import json
import struct
import sys
import zlib
from pathlib import Path
from unittest.mock import patch

from PIL import Image, TiffImagePlugin

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from engine.pictures_exchange import decode


def case(name, source, fmt, opacity=1.0, *, associated=False, **options):
    stream = io.BytesIO()
    source.save(stream, format=fmt, **options)
    data = stream.getvalue()
    if associated:
        # Pillow's TIFF writer always labels RGBA as unassociated. Mark these
        # deliberately premultiplied samples as associated in the first IFD.
        data = bytearray(data)
        endian = "<" if data[:2] == b"II" else ">"
        offset = struct.unpack_from(endian + "I", data, 4)[0]
        count = struct.unpack_from(endian + "H", data, offset)[0]
        for index in range(count):
            at = offset + 2 + index * 12
            if struct.unpack_from(endian + "H", data, at)[0] == 338:
                struct.pack_into(endian + "H", data, at + 8, 1)
                break
        else:
            raise ValueError("Missing TIFF alpha tag in fixture")
    pixels = decode(data, fmt)
    pixels.putalpha(pixels.getchannel("A").point([round(v * opacity) for v in range(256)]))
    return dict(
        name=name,
        format=fmt,
        opacity=opacity,
        data=base64.b64encode(data).decode(),
        width=pixels.width,
        height=pixels.height,
        rgba=list(pixels.tobytes()),
    )


def corpus():
    rgba = Image.new("RGBA", (17, 11))
    rgba.putdata(
        [
            (x * 13, y * 23, (x * 41 + y * 17) % 256, (x * 7 + y * 37) % 256)
            for y in range(11)
            for x in range(17)
        ]
    )
    cases = []
    for fmt, modes in [
        ("PNG", ["1", "L", "LA", "RGB", "RGBA", "P", "I;16"]),
        ("TIFF", ["1", "L", "LA", "RGB", "RGBA", "P", "CMYK", "I;16", "F"]),
        ("JPEG", ["L", "RGB", "CMYK"]),
        ("GIF", ["L", "P", "RGBA"]),
        ("BMP", ["1", "L", "RGB", "RGBA", "P"]),
    ]:
        for mode in modes:
            source = rgba.convert(mode)
            if mode == "I;16":
                source.putdata([0, 1, 254, 255, 256, 32767, 65535] * 26 + [42] * 5)
            if mode == "F":
                source.putdata([-1.0, 0.0, 0.5, 1.0, 127.7, 254.9, 255.0, 1000.0] * 23 + [42.0] * 3)
            for alpha in [1.0, 0.5, 0.0, 0.37]:
                cases.append(case(f"{fmt} {mode} alpha={alpha}", source, fmt, alpha))
    for fmt in ["PNG", "JPEG", "TIFF"]:
        for orientation in range(1, 9):
            exif = Image.Exif()
            exif[274] = orientation
            source = rgba.convert("RGB") if fmt == "JPEG" else rgba
            cases.append(case(f"{fmt} EXIF {orientation}", source, fmt, exif=exif))
    for fmt in ["GIF", "TIFF"]:
        cases.append(
            case(
                f"{fmt} first frame",
                rgba,
                fmt,
                save_all=True,
                append_images=[Image.new("RGBA", (17, 11), "white")],
            )
        )
    for mode in ["P", "LA", "I;16", "I;16B", "I", "F", "CMYK"]:
        for compression in ["raw", "tiff_lzw", "tiff_adobe_deflate", "packbits"]:
            cases.append(
                case(
                    f"TIFF {mode} {compression}",
                    rgba.convert(mode),
                    "TIFF",
                    compression=compression,
                )
            )
    cases.append(case("TIFF palette BigTIFF", rgba.convert("P"), "TIFF", big_tiff=True))
    premultiplied = Image.frombytes("RGBA", rgba.size, rgba.convert("RGBa").tobytes())
    cases.append(case("TIFF associated alpha", premultiplied, "TIFF", associated=True))
    for mode in ["RGB", "RGBA", "CMYK"]:
        for bits in [8, 16]:
            if mode == "CMYK" and bits == 16:
                continue
            for planar in [False, True]:
                cases.append(planar_tiff(mode, bits, planar))
    for kind, channels in [(0, 1), (2, 3), (4, 2), (6, 4)]:
        cases.append(png16(kind, channels))
    return cases


def png16(kind, channels):
    def chunk(name, value):
        return (
            struct.pack(">I", len(value))
            + name
            + value
            + struct.pack(">I", zlib.crc32(name + value))
        )

    width, height = 7, 3
    samples = [0, 1, 255, 256, 511, 32768, 65535]
    rows = bytearray()
    for y in range(height):
        rows.append(0)
        for x in range(width):
            for c in range(channels):
                rows.extend(struct.pack(">H", samples[(x + y + c * 3) % len(samples)]))
    data = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 16, kind, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(rows))
        + chunk(b"IEND", b"")
    )
    pixels = decode(data, "PNG")
    return dict(
        name=f"PNG 16-bit color type {kind}",
        format="PNG",
        opacity=1.0,
        data=base64.b64encode(data).decode(),
        width=pixels.width,
        height=pixels.height,
        rgba=list(pixels.tobytes()),
    )


def planar_tiff(mode, bits, planar):
    """Independent TIFF fixture with separate channel planes or chunky pixels."""
    width, height, channels = 7, 3, len(mode)
    tags = [
        (256, 4, [width]),
        (257, 4, [height]),
        (258, 3, [bits] * channels),
        (259, 3, [1]),
        (262, 3, [5 if mode == "CMYK" else 2]),
        (273, 4, [0] * (channels if planar else 1)),
        (277, 3, [channels]),
        (278, 4, [height]),
        (
            279,
            4,
            [width * height * bits // 8] * (channels)
            if planar
            else [width * height * bits // 8 * channels],
        ),
        (284, 3, [2 if planar else 1]),
    ]
    if mode == "RGBA":
        tags.append((338, 3, [2]))
    start = 8 + 2 + len(tags) * 12 + 4
    extra_size = sum(
        len(values) * (2 if kind == 3 else 4)
        for _, kind, values in tags
        if len(values) * (2 if kind == 3 else 4) > 4
    )
    plane_bytes = width * height * bits // 8
    tags = [
        (
            tag,
            kind,
            (
                [start + extra_size + p * plane_bytes for p in range(channels if planar else 1)]
                if tag == 273
                else values
            ),
        )
        for tag, kind, values in tags
    ]
    extra, entries = bytearray(), bytearray()
    for tag, kind, values in tags:
        payload = struct.pack("<" + ("H" if kind == 3 else "I") * len(values), *values)
        stored = (
            payload.ljust(4, b"\0") if len(payload) <= 4 else struct.pack("<I", start + len(extra))
        )
        entries.extend(struct.pack("<HHI", tag, kind, len(values)) + stored)
        if len(payload) > 4:
            extra.extend(payload)
    samples = [0, 1, 255, 256, 511, 32768, 65535] if bits == 16 else [0, 1, 31, 127, 128, 254, 255]
    values = [
        [samples[(p + c * 3) % len(samples)] for c in range(channels)]
        for p in range(width * height)
    ]
    ordered = (
        [row[c] for c in range(channels) for row in values]
        if planar
        else [v for row in values for v in row]
    )
    data = (
        b"II*\0"
        + struct.pack("<I", 8)
        + struct.pack("<H", len(tags))
        + entries
        + b"\0" * 4
        + extra
        + struct.pack("<" + ("H" if bits == 16 else "B") * len(ordered), *ordered)
    )
    if planar and bits == 16:
        # Pillow's default raw reader misreads these planar 16-bit samples.
        # Use its libtiff decoder and check against the independently authored
        # source values, rather than retaining the old decoder's lost channels.
        with patch.object(TiffImagePlugin, "READ_LIBTIFF", True):
            pixels = decode(data, "TIFF")
        expected = [
            tuple(v >> 8 for v in row) + ((255,) if mode == "RGB" else ()) for row in values
        ]
        assert list(pixels.getdata()) == expected
    else:
        pixels = decode(data, "TIFF")
    return dict(
        name=f"TIFF {mode} {bits}-bit planar={planar}",
        format="TIFF",
        opacity=1.0,
        data=base64.b64encode(data).decode(),
        width=pixels.width,
        height=pixels.height,
        rgba=list(pixels.tobytes()),
    )


if __name__ == "__main__":
    print(json.dumps(corpus(), allow_nan=False))
