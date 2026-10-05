"""Embedded raster pictures, with native size, rotation and lossless reflection.

The published embeddedobject format stores an unrotated BoundingBox and a
clockwise RotationAngle in degrees * 65536 (including in CDXML). Its PNG/TIFF/
JPEG/GIF/BMP properties are raw bytes in CDX and hexadecimal in CDXML.
"""

import base64
import io
import math
import warnings
import xml.etree.ElementTree as ET

MAX_BYTES = 16 * 1024 * 1024
MAX_PIXELS = 16_000_000
MAX_SIDE = 8192
FORMATS = ("PNG", "TIFF", "JPEG", "GIF", "BMP")


def decode(data, expected=None, budget=None):
    # Reference-only dependency. The application normalizes images in Rust.
    from PIL import Image, ImageOps

    if not data or len(data) > MAX_BYTES:
        raise ValueError("Embedded pictures must contain at most 16 MB")
    try:
        with warnings.catch_warnings():
            warnings.simplefilter("error", Image.DecompressionBombWarning)
            with Image.open(io.BytesIO(data), formats=list(FORMATS)) as source:
                if expected is not None and source.format != expected:
                    raise ValueError("Embedded picture bytes do not match their declared format")
                width, height = source.size
                if (
                    not 0 < width <= MAX_SIDE
                    or not 0 < height <= MAX_SIDE
                    or width * height > MAX_PIXELS
                ):
                    raise ValueError(
                        "Embedded pictures are limited to 8192 pixels per side and 16 million pixels"
                    )
                if budget is not None:
                    budget["pixels"] += width * height
                    if budget["pixels"] > 64_000_000:
                        raise ValueError("A drawing can contain at most 64 million picture pixels")
                # First frame only, matching file import. Normalize orientation
                # and transparency before crossing the worker boundary.
                return ImageOps.exif_transpose(source).convert("RGBA")
    except (
        OSError,
        SyntaxError,
        Image.DecompressionBombError,
        Image.DecompressionBombWarning,
    ) as e:
        raise ValueError("Invalid or oversized embedded picture") from e


def png_bytes(pixels):
    stream = io.BytesIO()
    pixels.save(stream, format="PNG")
    data = stream.getvalue()
    if len(data) > MAX_BYTES:
        raise ValueError("Decoded picture exceeds the 16 MB storage limit")
    return data


def read_picture(el, scale, budget=None, *, defer=False):
    name = next((name for name in FORMATS if el.get(name)), None)
    if name is None:
        raise ValueError(
            "Embedded picture needs a PNG, TIFF, JPEG, GIF or BMP representation; vector/OLE-only pictures are not supported"
        )
    value = el.get(name)
    if len(value) > MAX_BYTES * 3:
        raise ValueError("Embedded picture data exceeds 16 MB")
    try:
        data = bytes.fromhex(value)
    except ValueError as e:
        raise ValueError("Invalid embedded picture hexadecimal data") from e
    if not data or len(data) > MAX_BYTES:
        raise ValueError("Embedded pictures must contain at most 16 MB")
    opacity = float(el.get("alpha", "1"))
    if not math.isfinite(opacity) or not 0 <= opacity <= 1:
        raise ValueError("Invalid embedded picture opacity")
    box = [float(v) * scale for v in el.get("BoundingBox", "").split()]
    angle = float(el.get("RotationAngle", "0")) / 65536
    if len(box) != 4 or not all(math.isfinite(v) for v in box + [angle]):
        raise ValueError("Invalid embedded picture bounds or rotation")
    left, top, right, bottom = box
    width, height = right - left, bottom - top
    if not 0.01 <= abs(width) <= 1_000_000 or not 0.01 <= abs(height) <= 1_000_000:
        raise ValueError("Embedded picture dimensions are outside the supported range")
    c, s = math.cos(math.radians(angle)), math.sin(math.radians(angle))
    x, y = dict(x=c * width, y=s * width), dict(x=-s * height, y=c * height)
    origin = dict(x=(left + right - x["x"] - y["x"]) / 2, y=(top + bottom - x["y"] - y["y"]) / 2)
    result = dict(
        kind="picture",
        origin=origin,
        axis_x=x,
        axis_y=y,
    )
    if defer:
        result["picture_source"] = dict(
            data=base64.b64encode(data).decode("ascii"), format=name, opacity=opacity
        )
    else:
        pixels = decode(data, name, budget)
        if opacity != 1:
            pixels.putalpha(pixels.getchannel("A").point([round(v * opacity) for v in range(256)]))
        data = png_bytes(pixels)
        if budget is not None:
            budget["bytes"] += len(data)
            if budget["bytes"] > 64 * 1024 * 1024:
                raise ValueError("A drawing can contain at most 64 MB of encoded pictures")
        result["picture"] = base64.b64encode(data).decode("ascii")
    return result


def write_picture(parent, graphic, position, identifier, z, prepared_png=None):
    value = graphic.get("picture") if prepared_png is None else prepared_png
    if not isinstance(value, str) or len(value) > ((MAX_BYTES + 2) // 3) * 4:
        raise ValueError("Invalid embedded picture data")
    data = base64.b64decode(value, validate=True)
    origin, x, y = [graphic[key] for key in ("origin", "axis_x", "axis_y")]
    coords = [p[key] for p in (origin, x, y) for key in ("x", "y")]
    if not all(math.isfinite(v) for v in coords):
        raise ValueError("Invalid picture frame")
    width, height = math.hypot(x["x"], x["y"]), math.hypot(y["x"], y["y"])
    if (
        not 0.01 <= width <= 1_000_000
        or not 0.01 <= height <= 1_000_000
        or abs(x["x"] * y["x"] + x["y"] * y["y"]) > width * height * 0.0001
    ):
        raise ValueError("Picture exchange requires a rectangular frame")
    if prepared_png is None:
        from PIL import Image

        pixels = decode(data, "PNG")
        if x["x"] * y["y"] - x["y"] * y["x"] < 0:
            # Reference path: the format has no reflection flag.
            pixels = pixels.transpose(Image.Transpose.FLIP_TOP_BOTTOM)
        data = png_bytes(pixels)
    elif not data.startswith(b"\x89PNG\r\n\x1a\n"):
        raise ValueError("Prepared picture must be PNG")
    center = {k: origin[k] + (x[k] + y[k]) / 2 for k in ("x", "y")}
    a = dict(x=center["x"] - width / 2, y=center["y"] - height / 2)
    b = dict(x=center["x"] + width / 2, y=center["y"] + height / 2)
    angle = round(math.degrees(math.atan2(x["y"], x["x"])) * 65536)
    return ET.SubElement(
        parent,
        "embeddedobject",
        id=str(identifier),
        Z=str(z),
        BoundingBox=position(a) + " " + position(b),
        RotationAngle=str(angle),
        PNG=data.hex(),
    )
