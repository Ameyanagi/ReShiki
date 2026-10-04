"""Binary storage contracts extracted from source, without importing UNO.

These cover bytes, validation errors and stream cleanup. Real UNO persistence,
reopening and Save As remain the responsibility of the installed-extension tests.
"""

import ast
import base64
import json
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

SOURCE = Path(__file__).parents[1] / "extension" / "reshiki.py"
NATIVE = '{"version":18,"label":"α🧪","atoms":[],"bonds":[]}'.encode()
PNG = b"\x89PNG\r\n\x1a\n\x00\xffpreview"
LIMIT = 1024


def source_functions(source):
    tree = ast.parse(source)
    packet = next(
        node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == "packet"
    )
    embedded = next(
        node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == "Embedded"
    )
    read = next(
        node for node in embedded.body if isinstance(node, ast.FunctionDef) and node.name == "_read"
    )
    namespace = {"base64": base64, "json": json, "LIMIT": LIMIT}
    exec(compile(ast.Module(body=[packet, read], type_ignores=[]), str(SOURCE), "exec"), namespace)
    return namespace


def storage(entries=None, failures=None, counts=None, actions=None):
    entries = {
        "metadata.json": json.dumps({"version": 1, "extent": [100, 200]}).encode(),
        "drawing.rsk": NATIVE,
        "preview.png": PNG,
        **(entries or {}),
    }
    failures, counts, actions, events = failures or {}, counts or {}, actions or {}, []

    def record(phase, entry, *args):
        events.append((phase, entry, *args))
        if (phase, entry) in actions:
            actions[phase, entry]()
        if (phase, entry) in failures:
            raise failures[phase, entry]

    def open_stream(entry, mode):
        record("open", entry, mode)
        if entries[entry] is None:
            raise FileNotFoundError(entry)

        def get_input():
            record("input", entry)

            def read(ignored, limit):
                record("read", entry, ignored, limit)
                raw = entries[entry]
                count = counts.get(entry, len(raw) if hasattr(raw, "__len__") else 1)
                return count, SimpleNamespace(value=raw)

            return SimpleNamespace(readBytes=read, closeInput=lambda: record("close", entry))

        return SimpleNamespace(getInputStream=get_input)

    def open_storage(name, mode):
        record("storage", name, mode)
        return SimpleNamespace(
            openStreamElement=open_stream, dispose=lambda: record("dispose", name)
        )

    return SimpleNamespace(openStorageElement=open_storage), events


def stream_events(entry):
    return [
        ("open", entry, 1),
        ("input", entry),
        ("read", entry, None, LIMIT + 1),
        ("close", entry),
    ]


class PacketStorageTests(unittest.TestCase):
    def setUp(self):
        self.functions = source_functions(SOURCE.read_text())
        self.owner = SimpleNamespace(native=b"old native", png=b"old preview", extent=(1, 2))

    def read(self, **kwargs):
        parent, events = storage(**kwargs)
        try:
            self.functions["_read"](self.owner, parent, "Object 1")
        finally:
            self.events = events

    def expected_events(self, *entries):
        return [
            ("storage", "Object 1", 1),
            *(event for entry in entries for event in stream_events(entry)),
            ("dispose", "Object 1"),
        ]

    def unchanged(self):
        self.assertEqual(
            vars(self.owner), {"native": b"old native", "png": b"old preview", "extent": (1, 2)}
        )

    def test_binary_streams_preserve_bytes_and_order_without_base64_conversion(self):
        with (
            patch.object(base64, "b64encode", side_effect=AssertionError("unexpected encode")),
            patch.object(base64, "b64decode", side_effect=AssertionError("unexpected decode")),
        ):
            self.read()
        self.assertEqual(vars(self.owner), {"native": NATIVE, "png": PNG, "extent": (100, 200)})
        self.assertEqual(
            self.events, self.expected_events("metadata.json", "drawing.rsk", "preview.png")
        )

    def test_bytes_like_uno_values_still_become_immutable_bytes(self):
        class BytesSubclass(bytes):
            pass

        for convert in (bytearray, memoryview, BytesSubclass):
            with self.subTest(convert=convert):
                self.read(entries={"drawing.rsk": convert(NATIVE), "preview.png": convert(PNG)})
                self.assertIs(type(self.owner.native), bytes)
                self.assertIs(type(self.owner.png), bytes)
                self.assertEqual((self.owner.native, self.owner.png), (NATIVE, PNG))
                self.assertEqual(
                    self.events, self.expected_events("metadata.json", "drawing.rsk", "preview.png")
                )

    def test_mutable_buffer_conversion_remains_after_stream_close(self):
        native, png = bytearray(NATIVE), bytearray(PNG)
        updated_native = NATIVE.replace(b'"version":18', b'"version":17')
        updated_png = PNG + b"updated"
        self.read(
            entries={"drawing.rsk": native, "preview.png": png},
            actions={
                ("close", "drawing.rsk"): lambda: native.__setitem__(slice(None), updated_native),
                ("close", "preview.png"): lambda: png.__setitem__(slice(None), updated_png),
            },
        )
        self.assertEqual((self.owner.native, self.owner.png), (updated_native, updated_png))
        self.assertIs(type(self.owner.native), bytes)
        self.assertIs(type(self.owner.png), bytes)
        self.assertEqual(
            self.events, self.expected_events("metadata.json", "drawing.rsk", "preview.png")
        )

    def test_version_png_extent_and_native_errors_keep_precedence_after_all_reads(self):
        cases = [
            (
                {"version": 2, "extent": [0, 1]},
                b"invalid native",
                b"invalid PNG",
                ValueError,
                "Unsupported ReShiki embedding version.",
            ),
            (
                {"version": 1, "extent": [0, 1]},
                b"invalid native",
                b"invalid PNG",
                ValueError,
                "The embedded preview is not PNG.",
            ),
            (
                {"version": 1, "extent": [0, 1]},
                b"invalid native",
                PNG,
                ValueError,
                "Invalid physical drawing size.",
            ),
            (
                {"version": 1, "extent": [True, 1]},
                NATIVE,
                PNG,
                ValueError,
                "Invalid physical drawing size.",
            ),
            (
                {"version": 1, "extent": [2**31, 1]},
                NATIVE,
                PNG,
                ValueError,
                "Invalid physical drawing size.",
            ),
            (
                {"version": 1, "extent": [1, 1]},
                b"[]",
                PNG,
                ValueError,
                "Invalid native drawing data.",
            ),
            (
                {"version": 1, "extent": [1, 1]},
                b"",
                PNG,
                ValueError,
                "Embedded drawing or preview exceeds 64 MB.",
            ),
            (
                {"version": 1, "extent": [1, 1]},
                NATIVE,
                b"",
                ValueError,
                "Embedded drawing or preview exceeds 64 MB.",
            ),
        ]
        for metadata, native, png, error, message in cases:
            with self.subTest(metadata=metadata, native=native, png=png):
                with self.assertRaises(error) as caught:
                    self.read(
                        entries={
                            "metadata.json": json.dumps(metadata).encode(),
                            "drawing.rsk": native,
                            "preview.png": png,
                        }
                    )
                self.assertEqual(str(caught.exception), message)
                self.assertEqual(
                    self.events, self.expected_events("metadata.json", "drawing.rsk", "preview.png")
                )
                self.unchanged()

    def test_native_json_error_is_unchanged_and_does_not_replace_the_old_drawing(self):
        with self.assertRaises(json.JSONDecodeError) as caught:
            self.read(entries={"drawing.rsk": b"not JSON"})
        self.assertEqual(str(caught.exception), "Expecting value: line 1 column 1 (char 0)")
        self.assertEqual(
            self.events, self.expected_events("metadata.json", "drawing.rsk", "preview.png")
        )
        self.unchanged()

    def test_metadata_failures_stop_before_native_or_preview_acquisition(self):
        for metadata, error, message in (
            (b"not JSON", json.JSONDecodeError, "Expecting value: line 1 column 1 (char 0)"),
            (b'{"extent":[100,200]}', KeyError, "'version'"),
            (b'{"version":1}', KeyError, "'extent'"),
        ):
            with self.subTest(metadata=metadata):
                with self.assertRaises(error) as caught:
                    self.read(entries={"metadata.json": metadata})
                self.assertEqual(str(caught.exception), message)
                self.assertEqual(self.events, self.expected_events("metadata.json"))
                self.unchanged()

    def test_metadata_types_keep_json_decoder_behavior(self):
        metadata = json.dumps({"version": 1, "extent": [100, 200]})
        for value in (metadata, bytearray(metadata.encode())):
            with self.subTest(value=value):
                self.read(entries={"metadata.json": value})
                self.assertEqual(
                    vars(self.owner), {"native": NATIVE, "png": PNG, "extent": (100, 200)}
                )
        for value, name in ((123, "int"), (memoryview(metadata.encode()), "memoryview")):
            with self.subTest(value=value):
                self.owner = SimpleNamespace(
                    native=b"old native", png=b"old preview", extent=(1, 2)
                )
                with self.assertRaises(TypeError) as caught:
                    self.read(entries={"metadata.json": value})
                self.assertEqual(
                    str(caught.exception),
                    f"the JSON object must be str, bytes or bytearray, not {name}",
                )
                self.assertEqual(self.events, self.expected_events("metadata.json"))
                self.unchanged()

    def test_oversized_stream_stops_and_closes_before_reading_the_next_part(self):
        for entry in ("metadata.json", "drawing.rsk", "preview.png"):
            with self.subTest(entry=entry):
                with self.assertRaises(ValueError) as caught:
                    self.read(counts={entry: LIMIT + 1})
                self.assertEqual(str(caught.exception), "Embedded data exceeds 64 MB.")
                entries = ("metadata.json", "drawing.rsk", "preview.png")
                self.assertEqual(
                    self.events, self.expected_events(*entries[: entries.index(entry) + 1])
                )
                self.unchanged()

    def test_actual_oversized_binary_values_are_validated_even_if_count_is_underreported(self):
        for entry in ("drawing.rsk", "preview.png"):
            with self.subTest(entry=entry):
                with self.assertRaises(ValueError) as caught:
                    self.read(entries={entry: b"x" * (LIMIT + 1)}, counts={entry: 1})
                self.assertEqual(
                    str(caught.exception), "Embedded drawing or preview exceeds 64 MB."
                )
                self.assertEqual(
                    self.events, self.expected_events("metadata.json", "drawing.rsk", "preview.png")
                )
                self.unchanged()

    def test_invalid_uno_value_types_fail_before_preview_and_keep_conversion_errors(self):
        for value, error, message in (
            ("text", TypeError, "a bytes-like object is required, not 'str'"),
            (123, TypeError, "a bytes-like object is required, not 'int'"),
            (
                memoryview(NATIVE)[::2],
                BufferError,
                "memoryview: underlying buffer is not C-contiguous",
            ),
        ):
            with self.subTest(value=value):
                with self.assertRaises(error) as caught:
                    self.read(entries={"drawing.rsk": value})
                self.assertEqual(str(caught.exception), message)
                self.assertEqual(self.events, self.expected_events("metadata.json", "drawing.rsk"))
                self.unchanged()

    def test_open_and_input_failures_only_dispose_acquired_storage(self):
        for phase in ("open", "input"):
            with self.subTest(phase=phase):
                failure = RuntimeError("cannot acquire preview")
                with self.assertRaises(RuntimeError) as caught:
                    self.read(failures={(phase, "preview.png"): failure})
                self.assertIs(caught.exception, failure)
                expected = self.expected_events("metadata.json", "drawing.rsk")[:-1]
                expected += [("open", "preview.png", 1)]
                if phase == "input":
                    expected += [("input", "preview.png")]
                self.assertEqual(self.events, expected + [("dispose", "Object 1")])
                self.unchanged()
        failure = RuntimeError("cannot acquire storage")
        with self.assertRaises(RuntimeError) as caught:
            self.read(failures={("storage", "Object 1"): failure})
        self.assertIs(caught.exception, failure)
        self.assertEqual(self.events, [("storage", "Object 1", 1)])

    def test_missing_stream_and_read_failures_do_not_assign_partial_results(self):
        for entry in ("metadata.json", "drawing.rsk", "preview.png"):
            with self.subTest(entry=entry):
                with self.assertRaises(FileNotFoundError) as caught:
                    self.read(entries={entry: None})
                self.assertEqual(str(caught.exception), entry)
                self.assertEqual(self.events[-2:], [("open", entry, 1), ("dispose", "Object 1")])
                self.unchanged()
                failure = RuntimeError("read failed")
                with self.assertRaises(RuntimeError) as caught:
                    self.read(failures={("read", entry): failure})
                self.assertIs(caught.exception, failure)
                self.assertEqual(self.events[-2:], [("close", entry), ("dispose", "Object 1")])
                self.unchanged()

    def test_close_and_dispose_failures_keep_their_exception_precedence(self):
        close, dispose = RuntimeError("close failed"), RuntimeError("dispose failed")
        for failures, entries, expected in (
            (
                {
                    ("read", "drawing.rsk"): RuntimeError("read failed"),
                    ("close", "drawing.rsk"): close,
                },
                {},
                close,
            ),
            ({("close", "drawing.rsk"): close}, {"drawing.rsk": "wrong type"}, close),
            ({("close", "drawing.rsk"): close, ("dispose", "Object 1"): dispose}, {}, dispose),
            ({("dispose", "Object 1"): dispose}, {"drawing.rsk": b"invalid JSON"}, dispose),
        ):
            with self.subTest(failures=failures, entries=entries):
                with self.assertRaises(RuntimeError) as caught:
                    self.read(entries=entries, failures=failures)
                self.assertIs(caught.exception, expected)
                self.assertEqual(self.events[-1], ("dispose", "Object 1"))
                self.unchanged()

    def test_dispose_failure_after_validation_does_not_undo_successful_assignment(self):
        failure = RuntimeError("dispose failed")
        with self.assertRaises(RuntimeError) as caught:
            self.read(failures={("dispose", "Object 1"): failure})
        self.assertIs(caught.exception, failure)
        self.assertEqual(vars(self.owner), {"native": NATIVE, "png": PNG, "extent": (100, 200)})

    def test_worker_packets_still_require_base64_and_do_not_take_a_binary_field_as_a_mode(self):
        value = {
            "version": 1,
            "native": base64.b64encode(NATIVE).decode(),
            "png": base64.b64encode(PNG).decode(),
            "extent": [100, 200],
            "binary": True,
        }
        self.assertEqual(self.functions["packet"](value), (NATIVE, PNG, (100, 200)))
        with self.assertRaises(base64.binascii.Error) as caught:
            self.functions["packet"]({**value, "native": NATIVE, "png": PNG})
        self.assertEqual(str(caught.exception), "Only base64 data is allowed")
        with self.assertRaises(ValueError) as caught:
            self.functions["packet"]({**value, "version": 2, "native": "!", "png": "!"})
        self.assertEqual(str(caught.exception), "Unsupported ReShiki embedding version.")


if __name__ == "__main__":
    unittest.main()
