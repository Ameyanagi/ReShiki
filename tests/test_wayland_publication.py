"""Reject unaccepted Wayland fixture publication before exercising ReShiki Cut."""

import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "native/linux/tests"))
from wayland_session import Session, publication_receipt, verify_publication

MARKER = "application/x-reshiki-qa-publication-test"


def request(marker=MARKER):
    return (
        f'[ 626444.076]  -> wl_data_source@50.offer("{marker}")\n'
        "[ 626444.086]  -> wl_data_device@29.set_selection(wl_data_source@50, 35)\n"
    )


def accepted(marker=MARKER):
    return (
        request(marker)
        + "[ 626445.001] wl_data_device@29.data_offer(new id wl_data_offer@4278190081)\n"
        + f'[ 626445.002] wl_data_offer@4278190081.offer("{marker}")\n'
        + "[ 626445.003] wl_data_device@29.selection(wl_data_offer@4278190081)\n"
    )


class PublicationReceiptTests(unittest.TestCase):
    def test_requires_incoming_marker_on_selected_offer_from_requested_device(self):
        self.assertEqual(
            publication_receipt(accepted(), MARKER),
            {"device": "29", "source": "50", "serial": 35, "offer": "4278190081", "marker": MARKER},
        )

    def test_local_request_sync_or_unselected_marker_is_not_acceptance(self):
        cases = [
            request(),
            request() + "[ 626445.001] wl_callback@51.done(35)\n",
            accepted().replace("wl_data_device@29.selection", " -> wl_data_device@29.selection"),
            accepted().replace("wl_data_offer@4278190081)", "wl_data_offer@4278190082)"),
            accepted().replace("wl_data_device@29.data_offer", "wl_data_device@30.data_offer"),
        ]
        for protocol in cases:
            with self.subTest(protocol=protocol):
                self.assertIsNone(publication_receipt(protocol, MARKER))

    def test_other_generation_and_reused_offer_id_do_not_acknowledge_marker(self):
        self.assertIsNone(publication_receipt(accepted("old-marker"), MARKER))
        reused = (
            accepted()
            + "[ 626446.001] wl_data_device@29.data_offer(new id wl_data_offer@4278190081)\n"
            + '[ 626446.002] wl_data_offer@4278190081.offer("text/plain")\n'
            + "[ 626446.003] wl_data_device@29.selection(wl_data_offer@4278190081)\n"
        )
        self.assertIsNone(publication_receipt(reused, MARKER))

    def test_cancelled_source_never_acknowledges_publication(self):
        for protocol in (request(), accepted()):
            with (
                self.subTest(protocol=protocol),
                self.assertRaisesRegex(AssertionError, "publication cancelled"),
            ):
                publication_receipt(
                    protocol + "[ 626463.190] wl_data_source@50.cancelled()\n", MARKER
                )

    def test_reader_must_verify_marker_and_every_payload(self):
        expected = {MARKER: "marker-hash", "application/x-reshiki-drawing+json": "native-hash"}
        result = {
            "formats": list(expected),
            **{mime: {"mime": mime, "sha256": digest} for mime, digest in expected.items()},
        }
        verify_publication(result, expected)
        for mime in expected:
            with (
                self.subTest(mime=mime),
                self.assertRaisesRegex(AssertionError, "payload mismatch"),
            ):
                verify_publication(result | {mime: {"sha256": "previous-clipboard"}}, expected)
        with self.assertRaisesRegex(AssertionError, "MIME missing"):
            verify_publication(result | {"formats": [MARKER]}, expected)


class PublicationPreconditionTests(unittest.TestCase):
    def session(self, directory, *, cancelled=False, wrong_bytes=False, missing_receipt=False):
        session = Session.__new__(Session)
        session.out = directory
        publisher = Mock(pid=123)
        publisher.poll.return_value = None
        session.consumer = publisher
        protocol = directory / "publisher.log"
        protocol.write_text("")
        session.protocol_logs = {publisher.pid: protocol}
        session.publications = []
        session.command_id = 4
        session.begin_gated_cut = Mock()
        payload = directory / "foreign.json"
        payload.write_bytes(b"foreign fixture bytes")
        items = {"application/x-reshiki-drawing+json": str(payload)}
        calls = []

        def clipboard(operation="read", items=None, marker=None, mimes=None):
            calls.append(operation)
            if operation == "publish":
                self.assertIs(session.consumer, publisher)
                log = request(marker) if cancelled or missing_receipt else accepted(marker)
                if cancelled:
                    log += "[ 626463.190] wl_data_source@50.cancelled()\n"
                protocol.write_text(log)
                return {"active": True, "queued": True, "marker": marker}
            expected = session.publications[-1]["expected_sha256"]
            self.assertEqual(mimes, list(expected))
            return {
                "formats": mimes,
                **{
                    mime: {"mime": mime, "sha256": "stale" if wrong_bytes else digest}
                    for mime, digest in expected.items()
                },
            }

        def receiver(name):
            # Only the incoming selection receipt permits the focus transfer.
            self.assertTrue(session.publications[-1]["selection_receipt"])
            calls.append("new-reader")
            session.consumer = Mock(pid=456)

        session.clipboard = Mock(side_effect=clipboard)
        session.receiver = Mock(side_effect=receiver)
        return session, items, calls

    def test_cancelled_publication_refuses_before_reader_or_cut_and_retains_failure(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            session, items, calls = self.session(directory, cancelled=True)
            with self.assertRaisesRegex(AssertionError, "publication cancelled"):
                session.publish(items)
                session.begin_gated_cut("request")
            self.assertEqual(calls, ["publish"])
            session.receiver.assert_not_called()
            session.begin_gated_cut.assert_not_called()
            receipt = json.loads((directory / "publication-1.json").read_text())
            self.assertIn("publication cancelled", receipt["failure"])
            self.assertTrue(receipt["queued"]["queued"])
            self.assertNotIn("verified", receipt)

    def test_missing_receipt_times_out_before_reader_or_cut(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            session, items, _calls = self.session(directory, missing_receipt=True)
            with (
                patch("wayland_session.time.monotonic", side_effect=[0, 0, 10]),
                patch("wayland_session.time.sleep"),
                self.assertRaisesRegex(AssertionError, "Timed out.*selection receipt"),
            ):
                session.publish(items)
                session.begin_gated_cut("request")
            session.receiver.assert_not_called()
            session.begin_gated_cut.assert_not_called()
            self.assertIn(
                "Timed out", json.loads((directory / "publication-1.json").read_text())["failure"]
            )

    def test_reader_payload_mismatch_refuses_before_cut_and_retains_read(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            session, items, calls = self.session(directory, wrong_bytes=True)
            with self.assertRaisesRegex(AssertionError, "payload mismatch"):
                session.publish(items)
                session.begin_gated_cut("request")
            self.assertEqual(calls, ["publish", "new-reader", "read"])
            session.begin_gated_cut.assert_not_called()
            receipt = json.loads((directory / "publication-1.json").read_text())
            self.assertIn("independent_read", receipt)
            self.assertIn("payload mismatch", receipt["failure"])
            self.assertNotIn("verified", receipt)

    def test_receipt_precedes_reader_marker_hash_and_cut(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            session, items, calls = self.session(directory)
            session.publish(items)
            session.begin_gated_cut("request")
            self.assertEqual(calls, ["publish", "new-reader", "read"])
            session.begin_gated_cut.assert_called_once_with("request")
            receipt = json.loads((directory / "publication-1.json").read_text())
            self.assertTrue(receipt["verified"])
            marker = receipt["marker"]
            self.assertEqual(
                receipt["expected_sha256"][marker], hashlib.sha256(marker.encode()).hexdigest()
            )


if __name__ == "__main__":
    unittest.main()
