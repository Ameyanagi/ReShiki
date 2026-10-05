"""JSON-lines framing and packet ownership at the persistent worker boundary."""

import io
import json
import unittest
import weakref
from types import SimpleNamespace
from unittest.mock import patch

from engine import worker


class WorkerProtocolTests(unittest.TestCase):
    def test_responses_keep_exact_framing_and_recover_after_request_errors(self):
        def handle(request):
            if request.get("operation") == "fail":
                raise ValueError("Handler rejected the request")
            return {"output": "分子", "empty": None, "values": [1, 1.25]}

        output = io.StringIO()
        lines = io.StringIO('not json\n[]\n{"id": 7, "operation": "fail"}\n{"id": 8}\n{"id": 9}\n')
        with patch.object(worker, "sys", SimpleNamespace(stdin=lines)):
            with patch.object(worker, "handle", handle), patch("sys.stdout", output):
                worker.main()
        self.assertEqual(
            output.getvalue(),
            '{"id": null, "ok": false, "error": "Expecting value: line 1 column 1 (char 0)"}\n'
            '{"id": null, "ok": false, "error": "Request must be a JSON object"}\n'
            '{"id": 7, "ok": false, "error": "Handler rejected the request"}\n'
            '{"id": 8, "ok": true, "result": {"output": "\\u5206\\u5b50", '
            '"empty": null, "values": [1, 1.25]}}\n'
            '{"id": 9, "ok": true, "result": {"output": "\\u5206\\u5b50", '
            '"empty": null, "values": [1, 1.25]}}\n',
        )

    def test_completed_packets_are_released_before_next_read(self):
        class Line(str):
            pass

        class Packet(dict):
            pass

        references = []
        flushed = False
        test = self

        class Input:
            count = 0

            def __iter__(self):
                return self

            def __next__(self):
                nonlocal flushed
                if self.count:
                    test.assertTrue(flushed)
                    test.assertTrue(all(reference() is None for reference in references))
                    references.clear()
                    flushed = False
                if self.count == 3:
                    raise StopIteration
                self.count += 1
                line = Line(json.dumps({"id": self.count}) + "\n")
                references.append(weakref.ref(line))
                return line

        class Output(io.StringIO):
            def flush(self):
                nonlocal flushed
                # Packets must stay alive until the response has been flushed.
                test.assertTrue(all(reference() is not None for reference in references))
                flushed = True
                super().flush()

        def loads(line):
            request = Packet(json.loads(line))
            references.append(weakref.ref(request))
            return request

        def handle(request):
            if request["id"] == 2:
                raise ValueError("Rejected")
            result = Packet(output="payload")
            references.append(weakref.ref(result))
            return result

        output = Output()
        with patch.object(worker, "sys", SimpleNamespace(stdin=Input())):
            with patch.object(worker, "json", SimpleNamespace(loads=loads, dumps=json.dumps)):
                with patch.object(worker, "handle", handle), patch("sys.stdout", output):
                    worker.main()
        self.assertEqual(
            [json.loads(line)["ok"] for line in output.getvalue().splitlines()], [True, False, True]
        )


if __name__ == "__main__":
    unittest.main()
