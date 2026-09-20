"""Configure the Moruno GitHub signing environment without logging credentials."""

import argparse
import base64
import getpass
import json
import subprocess
from pathlib import Path

REPOSITORY = "Ameyanagi/moruno"
ENVIRONMENT = "macos-signing"


def gh(arguments, value=None):
    result = subprocess.run(["gh", *arguments], input=value, capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError(
            "GitHub signing configuration failed; check gh auth status and repository access"
        )
    return result.stdout


def configure_environment():
    endpoint = f"repos/{REPOSITORY}/environments/{ENVIRONMENT}"
    gh(
        ["api", "--method", "PUT", endpoint, "--input", "-"],
        json.dumps(
            {
                "deployment_branch_policy": {
                    "protected_branches": False,
                    "custom_branch_policies": True,
                },
            }
        ),
    )
    policies = json.loads(gh(["api", endpoint + "/deployment-branch-policies"]))
    existing = {(item["name"], item["type"]) for item in policies["branch_policies"]}
    for name, kind in [("main", "branch"), ("v*", "tag")]:
        if (name, kind) not in existing:
            gh(
                [
                    "api",
                    "--method",
                    "POST",
                    endpoint + "/deployment-branch-policies",
                    "--input",
                    "-",
                ],
                json.dumps({"name": name, "type": kind}),
            )


def set_secret(name, value):
    if not value.strip():
        raise ValueError(f"{name} cannot be empty")
    gh(["secret", "set", name, "--repo", REPOSITORY, "--env", ENVIRONMENT], value)
    print(f"Configured {name} (value hidden)")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--certificate", type=Path, help="Existing Developer ID Application .p12")
    parser.add_argument("--password-file", type=Path, help="Existing .p12 password file")
    parser.add_argument("--certificate-only", action="store_true")
    args = parser.parse_args()
    if args.certificate_only and not args.certificate:
        parser.error("--certificate-only requires --certificate")
    configure_environment()
    if args.certificate:
        password = (
            args.password_file.read_text(encoding="utf-8").strip()
            if args.password_file
            else getpass.getpass("Certificate export password: ")
        )
        team = input("Apple team ID: ").strip()
        identity = input("Developer ID Application signing identity: ").strip()
        if not team or not identity.startswith("Developer ID Application: "):
            raise ValueError("Provide the team ID and full Developer ID Application identity")
        set_secret(
            "MACOS_CERTIFICATE_P12_BASE64", base64.b64encode(args.certificate.read_bytes()).decode()
        )
        set_secret("MACOS_CERTIFICATE_PASSWORD", password)
        for name, value in [("APPLE_TEAM_ID", team), ("MACOS_SIGNING_IDENTITY", identity)]:
            gh(["variable", "set", name, "--repo", REPOSITORY, "--env", ENVIRONMENT], value)
            print(f"Configured {name}")
    if not args.certificate_only:
        apple_id = input("Apple ID used for notarization: ").strip()
        password = getpass.getpass("Apple app-specific password (hidden): ")
        set_secret("APPLE_ID", apple_id)
        set_secret("APPLE_APP_SPECIFIC_PASSWORD", password)
    print("Signing settings saved in GitHub's macos-signing environment.")


if __name__ == "__main__":
    main()
