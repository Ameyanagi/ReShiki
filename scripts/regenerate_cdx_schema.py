"""Refresh factual CDX tags/types/enumerations from the published format tables."""
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
import html
import json
import re
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
BASE = 'https://bobhanson.github.io/IUPAC-FAIRSpec/cdx_sdk/'
CACHE = ROOT / 'artifacts/clipboard-qa-20260920/spec'
CACHE.mkdir(parents=True, exist_ok=True)

def read(path):
    cache = CACHE / Path(path).name
    if not cache.exists():
        cache.write_bytes(urllib.request.urlopen(BASE + path, timeout=30).read())
    return cache.read_text(errors='replace')

def cells(page):
    for row in re.findall(r'<tr[^>]*>(.*?)</tr>', page, re.I | re.S):
        yield [html.unescape(re.sub('<[^>]*>', '', cell)).strip()
               for cell in re.findall(r'<td[^>]*>(.*?)</td>', row, re.I | re.S)]

def property_definition(row):
    code, name, kind, url = row
    url = url.replace('ArrowHead_', 'Arrowhead_')
    page = read(url)
    page = page[page.find('<h2>'):]
    enum = {}
    for values in cells(page):
        if len(values) == 2 and values[0] == 'CDXML Name:': name = values[1]
        if len(values) >= 3 and re.fullmatch(r'-?\d+|0x[0-9a-fA-F]+', values[0]):
            if values[1] and not values[1].startswith('('):
                enum[values[1]] = int(values[0], 16) if values[0].startswith('0x') else int(values[0])
    return code, (name, kind, enum)

rows = []
for row in re.findall(r'<tr[^>]*>(.*?)</tr>', read('TableOfProperties.htm'), re.I | re.S):
    values = next(cells('<tr>' + row + '</tr>'), [])
    if len(values) == 4 and values[0].startswith('0x'):
        url = re.search(r'href="(properties/[^"]+)"', row)
        if url: rows.append((int(values[0], 16), values[2], values[3], url.group(1)))
with ThreadPoolExecutor(max_workers=12) as pool:
    schema = dict(pool.map(property_definition, rows))
# Correct inconsistent spelling in the overview table from the individual pages.
for code, name in [(0x20e, 'MajorAxisEnd3D'), (0x20f, 'MinorAxisEnd3D'),
                   (0xa2f, 'ArrowheadType'), (0xa30, 'ArrowheadCenterSize'),
                   (0xa31, 'ArrowheadWidth'), (0xa35, 'ArrowheadHead'), (0xa36, 'ArrowheadTail')]:
    _, kind, enum = schema[code]
    schema[code] = (name, kind, enum)
# Current clipboard text uses the same style-run layout with UTF-8 text.
schema[0x709] = ('UTF8Text', 'CDXString', {})
output = ROOT / 'engine/cdx_schema.py'
output.write_text('"""Published CDX numeric schema. Regenerate with scripts/regenerate_cdx_schema.py.\n'
                  'Source: https://bobhanson.github.io/IUPAC-FAIRSpec/cdx_sdk/TableOfProperties.htm\n"""\n\n'
                  + 'PROPERTIES = {\n' + ''.join(f'    0x{code:04x}: {value!r},\n' for code, value in sorted(schema.items())) + '}\n')
print('Generated', len(schema), 'property definitions')
