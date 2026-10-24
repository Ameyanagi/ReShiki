"""Report which RSKPROBE markers survive in files (recursing into zips, PNG chunks, PDF/zlib streams).

usage: python3 scan.py PATH...   (files or directories)
"""
import io, re, sys, zipfile, zlib
from pathlib import Path

TAG = re.compile(rb"RSKPROBE-([A-Z]+-[A-Z]+) application/x-reshiki\+zlib;base64 ([A-Za-z0-9+/=]+) END-RSKPROBE-\1")


def inflations(data: bytes):
    """Yield data plus every zlib stream found inside it (PDF FlateDecode, PNG zTXt, ...)."""
    yield data
    for m in re.finditer(rb"stream\r?\n", data):
        try:
            yield zlib.decompressobj().decompress(data[m.end():m.end() + 50_000_000])
        except zlib.error:
            pass
    if data[:8] == b"\x89PNG\r\n\x1a\n":
        i = 8
        while i + 8 <= len(data):
            n = int.from_bytes(data[i:i + 4], "big")
            kind, body = data[i + 4:i + 8], data[i + 8:i + 8 + n]
            if kind == b"zTXt":
                try:
                    yield zlib.decompress(body[body.index(b"\0") + 2:])
                except Exception:
                    pass
            i += 12 + n


def found(data: bytes, name: str, out: dict):
    if data[:2] == b"PK":
        try:
            with zipfile.ZipFile(io.BytesIO(data)) as z:
                for info in z.infolist():
                    found(z.read(info), f"{name}!{info.filename}", out)
            return
        except zipfile.BadZipFile:
            pass
    for blob in inflations(data):
        if b"\\0" in blob:  # PDF literal strings escape '-' '/' '+' ';' as octal
            blob = re.sub(rb"\\([0-7]{3})", lambda m: bytes([int(m.group(1), 8)]), blob)
        for m in TAG.finditer(blob):
            intact = True
            try:
                zlib.decompress(__import__("base64").b64decode(m.group(2)))
            except Exception:
                intact = False
            out.setdefault(name, set()).add(m.group(1).decode() + ("" if intact else "(CORRUPT)"))


res: dict = {}
for arg in sys.argv[1:]:
    p = Path(arg)
    for f in ([p] if p.is_file() else sorted(x for x in p.rglob("*") if x.is_file())):
        found(f.read_bytes(), str(f.relative_to(p.parent) if p.is_dir() else f.name), res)
        res.setdefault(str(f.relative_to(p.parent) if p.is_dir() else f.name), set())
for k in sorted(res):
    print(f"{k}\t{', '.join(sorted(res[k])) or '-'}")
