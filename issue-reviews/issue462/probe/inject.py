# /// script
# dependencies = ["pypdf>=5"]
# ///
"""Embed probe markers (plus a real .rsk payload) into ReShiki PNG/SVG/PDF/EMF exports.

usage: uv run inject.py RSK IN_DIR OUT_DIR   (IN_DIR has drawing.png/.svg/.pdf/.emf)
Every marker is "RSKPROBE-<FORMAT>-<SLOT>"; scan.py looks for them.
"""
import base64, struct, sys, zlib
from pathlib import Path

from pypdf import PdfReader, PdfWriter
from pypdf.generic import DecodedStreamObject, NameObject

rsk, src, out = Path(sys.argv[1]).read_bytes(), Path(sys.argv[2]), Path(sys.argv[3])
out.mkdir(parents=True, exist_ok=True)
b64 = base64.b64encode(zlib.compress(rsk)).decode()


def payload(tag: str) -> str:
    return f"RSKPROBE-{tag} application/x-reshiki+zlib;base64 {b64} END-RSKPROBE-{tag}"


# PNG: one iTXt (uncompressed) and one zTXt (compressed) before IEND.
def chunk(kind: bytes, data: bytes) -> bytes:
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))


png = (src / "drawing.png").read_bytes()
iend = png.rindex(b"IEND") - 4
itxt = b"ReShiki\x00\x00\x00\x00\x00" + payload("PNG-ITXT").encode()
ztxt = b"ReShikiZ\x00\x00" + zlib.compress(payload("PNG-ZTXT").encode())
(out / "marked.png").write_bytes(png[:iend] + chunk(b"iTXt", itxt) + chunk(b"zTXt", ztxt) + png[iend:])

# SVG: a <metadata> element right after the root start tag.
svg = (src / "drawing.svg").read_text()
root_end = svg.index(">", svg.index("<svg")) + 1
meta = f'<metadata id="reshiki-drawing"><reshiki xmlns="https://reshiki.com/ns/drawing">{payload("SVG-META")}</reshiki></metadata>'
(out / "marked.svg").write_text(svg[:root_end] + meta + svg[root_end:])

# PDF: embedded file, XMP metadata stream, and a custom Info key.
w = PdfWriter(clone_from=PdfReader(src / "drawing.pdf"))
w.add_attachment("drawing.rsk", payload("PDF-EMBED").encode())
w.add_metadata({"/ReShikiDrawing": payload("PDF-INFO")})
xmp = DecodedStreamObject()
xmp.set_data(
    (
        '<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?><x:xmpmeta xmlns:x="adobe:ns:meta/">'
        '<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about=""'
        f' xmlns:rsk="https://reshiki.com/ns/xmp/"><rsk:drawing>{payload("PDF-XMP")}</rsk:drawing>'
        '</rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end="w"?>'
    ).encode()
)
xmp.update({NameObject("/Type"): NameObject("/Metadata"), NameObject("/Subtype"): NameObject("/XML")})
w._root_object[NameObject("/Metadata")] = w._add_object(xmp)
with open(out / "marked.pdf", "wb") as f:
    w.write(f)

# EMF: EMR_COMMENT (type 70) right after EMR_HEADER; patch header nBytes/nRecords.
emf = bytearray((src / "drawing.emf").read_bytes())
hsize = struct.unpack_from("<I", emf, 4)[0]
body = b"RSKD" + payload("EMF-COMMENT").encode()
body += b"\0" * (-len(body) % 4)
rec = struct.pack("<III", 70, 12 + len(body), len(body)) + body
emf[hsize:hsize] = rec
struct.pack_into("<I", emf, 48, struct.unpack_from("<I", emf, 48)[0] + len(rec))  # nBytes
struct.pack_into("<I", emf, 52, struct.unpack_from("<I", emf, 52)[0] + 1)  # nRecords
(out / "marked.emf").write_bytes(bytes(emf))

for p in sorted(out.iterdir()):
    print(p.name, p.stat().st_size)
print("rsk bytes", len(rsk), "payload b64 chars", len(b64))
