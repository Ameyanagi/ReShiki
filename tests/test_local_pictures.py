"""The application worker path must run without Pillow installed."""

import base64
import json
import subprocess
import sys
import unittest
from pathlib import Path

from engine.worker import handle

ROOT = Path(__file__).resolve().parents[1]
PNG = base64.b64decode(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg=="
)


class LocalPictureTests(unittest.TestCase):
    def test_worker_import_and_export_do_not_import_pillow(self):
        script = """
import builtins, json, sys
original = builtins.__import__
def checked(name, *args, **kwargs):
    if name == 'PIL' or name.startswith('PIL.'):
        raise AssertionError('Runtime attempted to import Pillow')
    return original(name, *args, **kwargs)
builtins.__import__ = checked
from engine.worker import handle
data = json.loads(sys.stdin.read())
xml = '<CDXML><page id="1"><embeddedobject id="2" BoundingBox="1 2 20 30" PNG="' + data['hex'] + '"/></page></CDXML>'
result = handle(dict(protocol=1, operation='import', format='cdxml', text=xml, local_pictures=True))
graphic = result['document']['graphics'][0]
source = graphic.pop('picture_source')
assert source['format'] == 'PNG' and source['opacity'] == 1.0
assert source['data'] == data['base64']
graphic['picture'] = data['base64']
result = handle(dict(protocol=1, operation='export', format='cdxml', document=result['document'],
    local_pictures=True, picture_exports={str(graphic['id']): data['base64']}))
assert 'embeddedobject' in result['output']
assert data['hex'] in result['output']
assert handle(dict(protocol=1, operation='import', format='smiles', text='CCO',
                   local_pictures=True))['analysis']['formula'] == 'C2H6O'
"""
        result = subprocess.run(
            [sys.executable, "-c", script],
            cwd=ROOT,
            input=json.dumps(dict(hex=PNG.hex(), base64=base64.b64encode(PNG).decode())),
            capture_output=True,
            text=True,
            timeout=30,
        )
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_local_mode_rejects_missing_exports_and_invalid_flags(self):
        request = dict(protocol=1, operation="import", format="smiles", text="CCO")
        for value in [None, "true", 1, []]:
            with self.assertRaisesRegex(ValueError, "boolean"):
                handle(dict(request, local_pictures=value))
        with self.assertRaisesRegex(ValueError, "object"):
            handle(dict(request, local_pictures=True, picture_exports=[]))
        request = dict(
            protocol=1,
            operation="import",
            format="cdxml",
            local_pictures=True,
            text=f'<CDXML><page id="1"><embeddedobject id="2" BoundingBox="1 2 20 30" PNG="{PNG.hex()}"/></page></CDXML>',
        )
        doc = handle(request)["document"]
        doc["graphics"][0].pop("picture_source")
        doc["graphics"][0]["picture"] = base64.b64encode(PNG).decode()
        with self.assertRaisesRegex(ValueError, "Missing prepared"):
            handle(
                dict(
                    protocol=1,
                    operation="export",
                    format="cdxml",
                    document=doc,
                    local_pictures=True,
                )
            )


if __name__ == "__main__":
    unittest.main()
