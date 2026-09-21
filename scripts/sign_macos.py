"""Developer ID signing and notarization using an ephemeral CI keychain."""

import base64
import json
import os
import secrets
import shlex
import subprocess
import tempfile
from contextlib import contextmanager
from pathlib import Path

SETTINGS = (
    "MACOS_CERTIFICATE_P12_BASE64",
    "MACOS_CERTIFICATE_PASSWORD",
    "APPLE_ID",
    "APPLE_APP_SPECIFIC_PASSWORD",
    "APPLE_TEAM_ID",
    "MACOS_SIGNING_IDENTITY",
)


def private_run(command):
    result = subprocess.run([str(arg) for arg in command], capture_output=True, text=True)
    if result.returncode:
        # Credential-bearing argv must never be exposed in exceptions or logs.
        raise RuntimeError(f"{command[0]} credential setup failed (exit {result.returncode})")
    return result.stdout


def run(command):
    return subprocess.run([str(arg) for arg in command], check=True)


@contextmanager
def signing_keychain(directory):
    missing = [name for name in SETTINGS if not os.environ.get(name)]
    if missing:
        raise ValueError("Missing signing configuration: " + ", ".join(missing))
    keychain = directory / "signing.keychain-db"
    p12 = directory / "identity.p12"
    p12.write_bytes(base64.b64decode(os.environ[SETTINGS[0]], validate=True))
    p12.chmod(0o600)
    password = secrets.token_urlsafe(48)
    original = shlex.split(private_run(["security", "list-keychains", "-d", "user"]))
    try:
        private_run(["security", "create-keychain", "-p", password, keychain])
        private_run(["security", "set-keychain-settings", "-lut", "7200", keychain])
        private_run(["security", "unlock-keychain", "-p", password, keychain])
        private_run(
            [
                "security",
                "import",
                p12,
                "-k",
                keychain,
                "-f",
                "pkcs12",
                "-P",
                os.environ["MACOS_CERTIFICATE_PASSWORD"],
                "-T",
                "/usr/bin/codesign",
            ]
        )
        private_run(
            [
                "security",
                "set-key-partition-list",
                "-S",
                "apple-tool:,apple:,codesign:",
                "-s",
                "-k",
                password,
                keychain,
            ]
        )
        private_run(["security", "list-keychains", "-d", "user", "-s", *original, keychain])
        identity = os.environ["MACOS_SIGNING_IDENTITY"]
        identities = private_run(["security", "find-identity", "-v", "-p", "codesigning", keychain])
        if (
            not identity.startswith("Developer ID Application: ")
            or f'"{identity}"' not in identities
        ):
            raise ValueError("Configured Developer ID Application identity not found")
        private_run(
            [
                "xcrun",
                "notarytool",
                "store-credentials",
                "reshiki-release",
                "--keychain",
                keychain,
                "--apple-id",
                os.environ["APPLE_ID"],
                "--team-id",
                os.environ["APPLE_TEAM_ID"],
                "--password",
                os.environ["APPLE_APP_SPECIFIC_PASSWORD"],
            ]
        )
        yield keychain
    finally:
        subprocess.run(
            ["security", "list-keychains", "-d", "user", "-s", *original], capture_output=True
        )
        subprocess.run(["security", "delete-keychain", str(keychain)], capture_output=True)
        p12.unlink(missing_ok=True)


def is_macho(path):
    if path.is_symlink() or not path.is_file():
        return False
    with path.open("rb") as stream:
        return stream.read(4) in {
            b"\xfe\xed\xfa\xce",
            b"\xce\xfa\xed\xfe",
            b"\xfe\xed\xfa\xcf",
            b"\xcf\xfa\xed\xfe",
            b"\xca\xfe\xba\xbe",
            b"\xbe\xba\xfe\xca",
            b"\xca\xfe\xba\xbf",
            b"\xbf\xba\xfe\xca",
        }


def verify_app(app):
    run(["codesign", "--verify", "--deep", "--strict", app])
    details = subprocess.run(
        ["codesign", "-d", "--verbose=4", str(app)], check=True, capture_output=True, text=True
    ).stderr
    if (
        f"TeamIdentifier={os.environ['APPLE_TEAM_ID']}\n" not in details
        or "Authority=Developer ID Application:" not in details
        or "(runtime)" not in details
        or "Timestamp=" not in details
    ):
        raise ValueError("Missing timestamped Developer ID signature with Hardened Runtime")
    run(["xcrun", "stapler", "validate", app])
    run(["spctl", "--assess", "--type", "execute", "--verbose=2", app])


def sign_and_notarize(app):
    with tempfile.TemporaryDirectory(prefix="reshiki-signing-") as temporary:
        directory = Path(temporary)
        with signing_keychain(directory) as keychain:
            command = [
                "codesign",
                "--force",
                "--sign",
                os.environ["MACOS_SIGNING_IDENTITY"],
                "--keychain",
                keychain,
                "--options",
                "runtime",
                "--timestamp",
            ]
            # Sign native helpers and libraries inside-out, then app bundles.
            # All dependencies have the same team, so library validation stays enabled.
            for entry in sorted(app.rglob("*"), key=lambda p: len(p.parts), reverse=True):
                if is_macho(entry) or (
                    not entry.is_symlink() and entry.suffix in {".app", ".framework"}
                ):
                    run([*command, entry])
            run([*command, app])
            run(["codesign", "--verify", "--deep", "--strict", app])
            submission = directory / "submission.zip"
            run(["ditto", "-c", "-k", "--keepParent", app, submission])
            notarize(submission, keychain)
            run(["xcrun", "stapler", "staple", app])
            verify_app(app)


def notarize(submission, keychain):
    auth = ["--keychain-profile", "reshiki-release", "--keychain", str(keychain)]
    response = subprocess.run(
        [
            "xcrun",
            "notarytool",
            "submit",
            str(submission),
            *auth,
            "--wait",
            "--timeout",
            "45m",
            "--output-format",
            "json",
        ],
        capture_output=True,
        text=True,
    )
    try:
        result = json.loads(response.stdout)
    except json.JSONDecodeError as error:
        raise RuntimeError("Notarization did not return a valid response") from error
    if response.returncode or result.get("status") != "Accepted":
        if result.get("id"):
            run(["xcrun", "notarytool", "log", result["id"], *auth])
        raise RuntimeError("Apple did not accept this notarization submission")
    print(f"Apple accepted notarization: {result['id']}")


def sign_disk_image(image):
    with tempfile.TemporaryDirectory(prefix="reshiki-dmg-signing-") as temporary:
        with signing_keychain(Path(temporary)) as keychain:
            run(
                [
                    "codesign",
                    "--force",
                    "--sign",
                    os.environ["MACOS_SIGNING_IDENTITY"],
                    "--keychain",
                    keychain,
                    "--timestamp",
                    image,
                ]
            )
            notarize(image, keychain)
            run(["xcrun", "stapler", "staple", image])
            run(["codesign", "--verify", "--strict", image])
            run(["xcrun", "stapler", "validate", image])
            run(
                [
                    "spctl",
                    "--assess",
                    "--type",
                    "open",
                    "--context",
                    "context:primary-signature",
                    "--verbose=2",
                    image,
                ]
            )
