"""Use the patched CA bundle for developer/build HTTPS downloads on every OS.

An explicit cafile avoids adding the Windows ROOT store via load_default_certs.
It does not replace Python 3.12.16's additional compromised-key denylist.
"""

import importlib
import re
import ssl

MINIMUM_CERTIFI = (2026, 10, 19)


def verified_context():
    # Keep cached/local and standard-library-only packaging paths independent of
    # certifi. Only an actual HTTPS download needs this development dependency.
    try:
        certifi = importlib.import_module("certifi")
    except ModuleNotFoundError as error:
        raise RuntimeError(
            "HTTPS downloads require certifi>=2026.10.19; use uv run --with certifi==2026.10.19"
        ) from error
    version = certifi.__version__
    if (
        not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version)
        or tuple(map(int, version.split("."))) < MINIMUM_CERTIFI
    ):
        raise RuntimeError(f"HTTPS downloads require certifi>=2026.10.19; found {version}")
    context = ssl.create_default_context(cafile=certifi.where())
    if context.verify_mode != ssl.CERT_REQUIRED or not context.check_hostname:
        raise RuntimeError("The download TLS context must verify certificates and hostnames")
    return context
