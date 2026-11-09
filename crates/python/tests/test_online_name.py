from __future__ import annotations

import io
import json
from http.client import HTTPException
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.request import Request

import pytest
import reshiki
from reshiki import _native, _online


class Response(io.BytesIO):
    status = 200


@pytest.fixture
def transport(monkeypatch: pytest.MonkeyPatch) -> tuple[list[Any], list[tuple[Request, float]]]:
    replies: list[Any] = []
    calls: list[tuple[Request, float]] = []

    def open_request(request: Request, *, timeout: float) -> Response:
        calls.append((request, timeout))
        reply = replies.pop(0)
        if isinstance(reply, Exception):
            raise reply
        return Response(reply if isinstance(reply, bytes) else json.dumps(reply).encode())

    monkeypatch.setattr(_online, "_OPEN", open_request)
    monkeypatch.setattr(_online, "sleep", lambda seconds: None)
    return replies, calls


def properties(smiles: str, cid: int = 702) -> dict[str, Any]:
    return {"PropertyTable": {"Properties": [{"CID": cid, "SMILES": smiles}]}}


def test_public_online_constructor_resolves_one_complete_encoded_name(transport: Any) -> None:
    replies, calls = transport
    replies.extend([{"IdentifierList": {"CID": [702, 702]}}, properties("CCO")])
    molecule = reshiki.Molecule.from_name("  (E)-a/b + #?% β  ", online=True)
    assert molecule.properties().formula == "C2H6O"
    assert calls[0][0].full_url == (
        "https://pubchem.ncbi.nlm.nih.gov/rest/pug/compound/name/"
        "%28E%29-a%2Fb%20%2B%20%23%3F%25%20%CE%B2/cids/JSON?name_type=complete"
    )
    assert calls[1][0].full_url.endswith("/cid/702/property/SMILES/JSON")
    assert all(timeout == 15 for _, timeout in calls)
    assert all(request.get_header("Accept") == "application/json" for request, _ in calls)


def test_offline_constructors_never_contact_pubchem(transport: Any) -> None:
    _, calls = transport
    assert reshiki.Molecule.from_name("ethanol").properties().formula == "C2H6O"
    assert reshiki.Molecule.from_name("ethanol", online=False).properties().formula == "C2H6O"
    assert calls == []


@pytest.mark.parametrize("online", [None, 1, "false"])
def test_online_requires_an_explicit_boolean(online: Any, transport: Any) -> None:
    _, calls = transport
    with pytest.raises(TypeError, match="online must be a boolean"):
        reshiki.Molecule.from_name("ethanol", online=online)
    assert calls == []


@pytest.mark.parametrize("name", ["", "  ", ".", "..", "ethanol\n", "a\x85b", "β" * 1025])
def test_invalid_names_are_rejected_before_network(name: str, transport: Any) -> None:
    _, calls = transport
    with pytest.raises(ValueError, match="chemical name"):
        _online.from_name(name)
    assert calls == []


def test_name_must_be_text(transport: Any) -> None:
    _, calls = transport
    with pytest.raises(TypeError, match="name must be a string"):
        _online.from_name(123)  # type: ignore[arg-type]
    assert calls == []


@pytest.mark.parametrize(
    ("reply", "message"),
    [
        ({"IdentifierList": {"CID": []}}, "no complete-name match"),
        ({"IdentifierList": {"CID": [702, 241]}}, "multiple PubChem structures"),
        ({"IdentifierList": {"CID": [0]}}, "invalid or incomplete"),
        ({"IdentifierList": {"CID": [True]}}, "invalid or incomplete"),
        ({"IdentifierList": {"CID": ["702"]}}, "invalid or incomplete"),
        ({"IdentifierList": {"CID": None}}, "invalid or incomplete"),
        ({"IdentifierList": []}, "invalid or incomplete"),
        ({}, "invalid or incomplete"),
        ([], "invalid or incomplete"),
        (b"<html>unavailable</html>", "invalid or incomplete"),
    ],
)
def test_cid_failures_do_not_select_a_structure(reply: Any, message: str, transport: Any) -> None:
    replies, calls = transport
    replies.append(reply)
    with pytest.raises(ValueError, match=message):
        _online.from_name("ethanol")
    assert len(calls) == 1


@pytest.mark.parametrize(
    "reply",
    [
        {},
        {"PropertyTable": []},
        {"PropertyTable": {"Properties": []}},
        {"PropertyTable": {"Properties": [{"CID": 241, "SMILES": "CCO"}]}},
        {"PropertyTable": {"Properties": [{"CID": 702, "ConnectivitySMILES": "CCO"}]}},
        {"PropertyTable": {"Properties": [{"CID": 702, "SMILES": "CCO"}] * 2}},
        {"PropertyTable": {"Properties": [None]}},
        properties(""),
        properties("CCO trailing name"),
    ],
)
def test_missing_wrong_or_multiple_property_records_fail(reply: Any, transport: Any) -> None:
    replies, _ = transport
    replies.extend([{"IdentifierList": {"CID": [702]}}, reply])
    with pytest.raises(ValueError, match="PubChem"):
        _online.from_name("ethanol")


@pytest.mark.parametrize(
    "smiles",
    ["[15NH3+][C@H](C)C(=O)[O-]", "F/C=C/Cl", "[Na+].CC(=O)[O-]", "[CH3]"],
)
def test_complete_charge_isotope_stereo_salt_and_radical_identity_survive(
    smiles: str, transport: Any
) -> None:
    replies, _ = transport
    replies.extend([{"IdentifierList": {"CID": [702]}}, properties(smiles)])
    result = reshiki.Molecule.from_name("exact chemical name", online=True)
    expected = reshiki.Molecule.from_smiles(smiles)
    assert result.smiles() == expected.smiles()
    assert result.properties().inchikey == expected.properties().inchikey


@pytest.mark.parametrize(
    "smiles",
    [
        "FC=[C@AL1]=CF",
        "F[Pt@SP1](Cl)(Br)I",
        "C/C=C=C=C/C",
        "C[C@H](O)C",
        "CC|r|",
        "CC>>CC",
        "C" * 513,
        "*CC",
        "not-a-smiles",
    ],
)
def test_unusable_structures_never_become_approximate_graphs(smiles: str, transport: Any) -> None:
    replies, _ = transport
    replies.extend([{"IdentifierList": {"CID": [702]}}, properties(smiles)])
    with pytest.raises(ValueError):
        _online.from_name("unusable structure")


def http_error(status: int, body: Any) -> HTTPError:
    raw = body if isinstance(body, bytes) else json.dumps(body).encode()
    return HTTPError("https://pubchem.ncbi.nlm.nih.gov/", status, "failure", {}, io.BytesIO(raw))


@pytest.mark.parametrize(
    ("failure", "exception", "message"),
    [
        (http_error(404, {"Fault": {"Code": "PUGREST.NotFound"}}), ValueError, "no complete-name"),
        (http_error(404, b"unavailable"), RuntimeError, "HTTP 404"),
        (http_error(404, {"Fault": 1}), RuntimeError, "HTTP 404"),
        (http_error(429, {}), RuntimeError, "busy or limiting"),
        (http_error(503, {}), RuntimeError, "busy or limiting"),
        (http_error(500, {}), RuntimeError, "HTTP 500"),
        (http_error(302, {}), RuntimeError, "HTTP 302"),
        (URLError("connection failed"), ConnectionError, "Check the connection"),
        (TimeoutError(), TimeoutError, "15-second network timeout"),
        (URLError(TimeoutError()), TimeoutError, "15-second network timeout"),
        (HTTPException("invalid response"), ConnectionError, "Check the connection"),
    ],
)
def test_transport_failures_are_actionable_without_retries(
    failure: Exception, exception: type[Exception], message: str, transport: Any
) -> None:
    replies, calls = transport
    replies.append(failure)
    with pytest.raises(exception, match=message):
        _online.from_name("ethanol")
    assert len(calls) == 1


def test_response_size_is_bounded(transport: Any) -> None:
    replies, _ = transport
    replies.append(b" " * (_online._LIMIT + 1))
    with pytest.raises(RuntimeError, match="64 KiB"):
        _online.from_name("ethanol")


def test_requests_are_rate_limited(monkeypatch: pytest.MonkeyPatch, transport: Any) -> None:
    replies, calls = transport
    pauses: list[float] = []
    monkeypatch.setattr(_online, "sleep", pauses.append)
    for _ in range(2):
        replies.extend([{"IdentifierList": {"CID": [702]}}, properties("CCO")])
        _online.from_name("ethanol")
    assert len(calls) == 4
    assert pauses == [0.25] * 4


def test_redirects_are_not_followed() -> None:
    assert (
        _online._NoRedirects().redirect_request(None, None, 302, "", {}, "http://example.com")
        is None
    )


def test_native_online_import_rejects_extensions_without_network() -> None:
    with pytest.raises(ValueError, match="ordinary molecular SMILES"):
        _native.parse_online_smiles("CC |r|")
