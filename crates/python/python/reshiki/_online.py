"""Explicit PubChem name lookup; ordinary name parsing never imports this module."""

from __future__ import annotations

import json
from http.client import HTTPException, HTTPResponse
from threading import Lock
from time import sleep
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.parse import quote
from urllib.request import HTTPRedirectHandler, Request, build_opener

from . import _native

_API = "https://pubchem.ncbi.nlm.nih.gov/rest/pug/compound"
_LIMIT = 65_536
_TIMEOUT = 15.0
_SLOT = Lock()
_INVALID = "PubChem returned an invalid or incomplete molecular response"
_NOT_FOUND = "PubChem found no complete-name match"


class _NoRedirects(HTTPRedirectHandler):
    def redirect_request(
        self, req: Any, fp: Any, code: int, msg: str, headers: Any, newurl: str
    ) -> None:
        return None


_OPEN = build_opener(_NoRedirects()).open


def _checked_name(name: str) -> str:
    if not isinstance(name, str):
        raise TypeError("name must be a string")
    if len(name.encode("utf-8")) > 2048 or any(
        ord(char) < 32 or 127 <= ord(char) <= 159 for char in name
    ):
        raise ValueError("Enter a chemical name of at most 2048 bytes without control characters")
    name = name.strip()
    if not name or name in {".", ".."}:
        raise ValueError("Enter a complete chemical name")
    return name


def _read(response: HTTPResponse | HTTPError) -> bytes:
    data = response.read(_LIMIT + 1)
    if len(data) > _LIMIT:
        raise RuntimeError("PubChem's response exceeded the 64 KiB lookup limit")
    return data


def _get_json(url: str) -> dict[str, Any]:
    # The enclosing lookup lock includes both requests and this pause, keeping
    # this process below PubChem's five-requests-per-second limit.
    sleep(0.25)
    request = Request(
        url,
        headers={
            "Accept": "application/json",
            "Accept-Encoding": "identity",
            "User-Agent": f"ReShiki/{_native.version()}",
        },
    )
    try:
        try:
            response = _OPEN(request, timeout=_TIMEOUT)
        except HTTPError as error:
            response = error
        with response:
            data = _read(response)
            status = response.status
    except (TimeoutError, URLError, OSError, HTTPException) as error:
        if isinstance(error, TimeoutError) or (
            isinstance(error, URLError) and isinstance(error.reason, TimeoutError)
        ):
            raise TimeoutError("PubChem lookup exceeded its 15-second network timeout") from error
        raise ConnectionError(
            "PubChem could not complete this lookup. Check the connection or try again later."
        ) from error
    if status != 200:
        if status == 404:
            try:
                fault = json.loads(data)
            except (ValueError, UnicodeDecodeError):
                fault = None
            detail = fault.get("Fault") if isinstance(fault, dict) else None
            if isinstance(detail, dict) and detail.get("Code") == "PUGREST.NotFound":
                raise ValueError(_NOT_FOUND)
        if status in {429, 503}:
            raise RuntimeError("PubChem is busy or limiting requests. Try this lookup again later.")
        raise RuntimeError(f"PubChem lookup failed (HTTP {status}); no structure was accepted")
    try:
        result = json.loads(data)
    except (ValueError, UnicodeDecodeError) as error:
        raise ValueError(_INVALID) from error
    if not isinstance(result, dict):
        raise ValueError(_INVALID)
    return result


def from_name(name: str) -> tuple[bytes, str]:
    """Return one complete PubChem graph through the native identity-checked importer.

    The name is sent only to PubChem. Each network operation has a 15-second
    timeout. Ambiguous matches and unusable structures raise ``ValueError``;
    service failures raise ``RuntimeError`` and connection failures raise
    ``ConnectionError`` or ``TimeoutError``. Returned salts retain all components.
    """
    name = _checked_name(name)
    with _SLOT:
        reply = _get_json(f"{_API}/name/{quote(name, safe='')}/cids/JSON?name_type=complete")
        identifiers = reply.get("IdentifierList")
        cids = identifiers.get("CID") if isinstance(identifiers, dict) else None
        if not isinstance(cids, list) or any(type(cid) is not int or cid <= 0 for cid in cids):
            raise ValueError(_INVALID)
        if not cids:
            raise ValueError(_NOT_FOUND)
        cid = cids[0]
        if any(other != cid for other in cids):
            raise ValueError(
                "This name matches multiple PubChem structures. Enter a more specific name; "
                "no structure was selected."
            )
        # SMILES includes stereo and isotopes. ConnectivitySMILES cannot be used
        # as a fallback: https://pubchem.ncbi.nlm.nih.gov/docs/pug-rest
        reply = _get_json(f"{_API}/cid/{cid}/property/SMILES/JSON")
    table = reply.get("PropertyTable")
    properties = table.get("Properties") if isinstance(table, dict) else None
    if not isinstance(properties, list) or len(properties) != 1:
        raise ValueError("PubChem did not return exactly one molecular structure")
    property = properties[0]
    if (
        not isinstance(property, dict)
        or type(property.get("CID")) is not int
        or property["CID"] != cid
    ):
        raise ValueError(_INVALID)
    smiles = property.get("SMILES")
    if not isinstance(smiles, str) or not smiles or any(char.isspace() for char in smiles):
        raise ValueError(_INVALID)
    return _native.parse_online_smiles(smiles)
