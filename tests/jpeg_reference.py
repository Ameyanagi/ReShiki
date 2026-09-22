"""Independent Pillow/libjpeg-turbo pixel corpus for chroma sampling boundaries."""

import base64
import io
import itertools
import json
import random
from pathlib import Path

import PIL
from PIL import Image, ImageOps, features

ROOT = Path(__file__).resolve().parents[1]


def emit(name, data, opacity=1.0):
    with Image.open(io.BytesIO(data)) as source:
        image = ImageOps.exif_transpose(source).convert("RGBA")
    image.putalpha(image.getchannel("A").point([round(v * opacity) for v in range(256)]))
    print(
        json.dumps(
            dict(
                name=name,
                data=base64.b64encode(data).decode(),
                width=image.width,
                height=image.height,
                rgba=base64.b64encode(image.tobytes()).decode(),
                opacity=opacity,
            )
        )
    )


def pattern(size, kind):
    image = Image.new("RGB", size)
    rng = random.Random(3419)
    colors = [(255, 0, 0), (0, 255, 0), (0, 0, 255), (0, 0, 0), (255, 255, 255), (255, 255, 0)]
    values = []
    for y in range(size[1]):
        for x in range(size[0]):
            if kind == "checkers":
                value = colors[(x + y * 3) % len(colors)]
            elif kind == "edges":
                value = colors[(int(x >= size[0] // 2) + 2 * int(y >= size[1] // 2)) % len(colors)]
            elif kind == "noise":
                value = tuple(rng.randrange(256) for _ in range(3))
            else:
                value = (
                    x * 251 // max(1, size[0] - 1),
                    y * 251 // max(1, size[1] - 1),
                    (x * 17 + y * 41) % 256,
                )
            values.append(value)
    image.putdata(values)
    return image


def main():
    print(
        json.dumps(
            dict(
                pillow=PIL.__version__,
                libjpeg=features.version("jpg"),
                libjpeg_turbo=features.version("libjpeg_turbo"),
            )
        )
    )
    emit("retained-3x2", (ROOT / "tests/fixtures/jpeg-subsampled-3x2.jpg").read_bytes())
    dimensions = [
        (1, 1),
        (1, 2),
        (2, 1),
        (2, 2),
        (3, 2),
        (4, 2),
        (3, 3),
        (4, 4),
        (5, 1),
        (5, 2),
        (5, 3),
        (5, 5),
        (6, 5),
        (7, 8),
        (8, 7),
        (8, 8),
        (9, 9),
        (15, 16),
        (16, 15),
        (16, 16),
        (17, 17),
        (31, 17),
        (32, 32),
        (33, 31),
        (63, 65),
    ]
    for size, sampling, kind, quality, progressive in itertools.product(
        dimensions, [0, 1, 2], ["checkers", "edges", "noise", "gradient"], [50, 95], [False, True]
    ):
        stream = io.BytesIO()
        pattern(size, kind).save(
            stream, format="JPEG", subsampling=sampling, quality=quality, progressive=progressive
        )
        emit(f"{size}/{sampling}/{kind}/{quality}/progressive={progressive}", stream.getvalue())
    for sampling, orientation, opacity in itertools.product(
        [0, 1, 2], range(1, 9), [0, 0.37, 0.5, 1]
    ):
        exif = Image.Exif()
        exif[274] = orientation
        stream = io.BytesIO()
        pattern((17, 9), "checkers").save(
            stream, format="JPEG", subsampling=sampling, quality=95, exif=exif
        )
        emit(f"orientation/{sampling}/{orientation}/{opacity}", stream.getvalue(), opacity)
    for mode, progressive in itertools.product(["L", "CMYK"], [False, True]):
        stream = io.BytesIO()
        pattern((17, 9), "checkers").convert(mode).save(
            stream, format="JPEG", quality=95, progressive=progressive
        )
        emit(f"preserved/{mode}/{progressive}", stream.getvalue())


if __name__ == "__main__":
    main()
