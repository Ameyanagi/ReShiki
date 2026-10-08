"""Run the agent API P1 acceptance gates that a development machine can check.

docs/agent-api-p1-validation.md defines gates G1-G8 and records their evidence.
With --local this runs the locally automatable subset in order: the G3-G7 cargo
and uv commands, then the G8 audit, diff and grep checks. It prints a pass/fail
table and every command line, and exits 1 if any check fails. A test command
that runs no test fails, so a mistyped filter cannot pass. CI and manual gates
are listed as pending evidence; this script never claims them.
"""

import argparse
import os
import re
import shlex
import subprocess
import sys
from collections.abc import Callable, Sequence
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RECORD = "docs/agent-api-p1-validation.md"
OUTPUT_TAIL = 40

# ops-1's byte pins of the Codex tool JSON and the Proposal and critique schemas.
CODEX_PIN_SUBJECT = "test(agent): pin Codex tool JSON, Proposal and critique schema bytes"
CODEX_PIN_TEST = "tests/assistant_contract.rs"
CODEX_PINS = tuple(
    f"tests/fixtures/agent-contract/{name}.json"
    for name in ("codex-dynamic-tools", "critique-schema", "proposal-schema")
)
# ops-3 changed only the expression that produces the pinned tool JSON.
OLD_CALL = "assistant::canvas_tools::definitions()"
NEW_CALL = "assistant::codex::dynamic_tools(&assistant::canvas_tools::SPECS)"

APP_PATHS = ("src/app.rs", "src/app", "src/canvas.rs", "src/canvas")
# Reviewed GUI source changes as (added, removed) lines: ops-4's characterized
# extractions, ops-9's pub(super) on two inspector helpers, and the
# registration of the ops parity test module.
REVIEWED_APP_SOURCES = {
    "src/app.rs": (2, 0),
    "src/app/figure_export.rs": (13, 16),
    "src/app/files.rs": (1, 3),
    "src/app/inspector.rs": (4, 19),
}
PARITY_REGISTRATION = ["#[cfg(test)]", "mod ops_parity_tests;"]
# #[cfg(test)] modules: the ops parity tests and the characterization tests.
REVIEWED_APP_TESTS = frozenset(
    {"src/app/ops_parity_tests.rs", "src/app/files/tests.rs", "src/app/inspector/tests.rs"}
)
# src/main.rs enters worker modes, then --mcp/--cli, then Windows Office
# handling, the engine check and the GUI, in this order.
DISPATCH_ORDER = (
    '"--geometry-worker"',
    '"--inchi-worker"',
    '"--print-worker"',
    "launch::mode(",
    '"--graphics-info"',
    "enable_office_embedding()",
    '"--engine-check"',
    "iced::application(",
)
LICENSE_CHECK = (
    'import json, subprocess, sys; from pathlib import Path; sys.path.insert(0, "scripts"); '
    "from license_notices import consolidated_notices; "
    "metadata = json.loads(subprocess.check_output("
    '["cargo", "metadata", "--format-version", "1", "--locked"])); '
    'print("Consolidated notices:", len(consolidated_notices(Path("."), metadata)), "bytes")'
)

# CI and manual evidence this script cannot produce, as (gate, evidence, source).
PENDING = (
    ("G1", "(a) transcripts and rmcp interop on 3 OSes", "checks.yml rust job"),
    ("G1", "(b) stdlib driver on 6 package targets", "release.yml nightly=true"),
    ("G1", "(c) phase-aware corpus on 3 OSes", "checks.yml rust job"),
    ("G1", "(d) Claude Code, Claude Desktop (macOS, Windows), Codex CLI", "owner"),
    ("G2", "6 build jobs green with verify_agent_api", "release.yml nightly=true"),
    ("G2", "quarantined launch of the signed app", "release.yml sign_macos=true on main"),
    ("G2", "Windows upgrade over a running server", "release.yml nightly=true"),
    ("G3", "escape suite on 3 OSes, RESHIKI_REQUIRE_LINK_TESTS=1", "checks.yml rust job"),
    ("G3", "home-folder grant refusal on windows-2022 (fix-1)", "checks.yml rust job"),
    ("G4", "Windows pipe EOF, release profile, x64 and ARM64", "release.yml nightly=true"),
    ("G4", "quota and cancellation tests on 3 OSes", "checks.yml rust job"),
    ("G5", "malformed corpus on 3 OSes", "checks.yml rust job"),
    ("G6", "audit inside release notices() on 6 targets", "release.yml nightly=true"),
    ("G7", "privacy-policy review sign-off", "owner"),
    ("G8", "full checks.yml on 3 OSes", "checks.yml on the tip"),
    ("G8", "live_reference=true, 12 shards", "checks.yml dispatch"),
    ("G8", "existing package checks unchanged", "release.yml nightly=true"),
    ("G8", "real-app GUI check on a debug build", "owner, codex-computer-use"),
    ("G8", "binary size delta per target", "release.yml nightly=true"),
)

TEST_RESULT = re.compile(
    r"^test result: \w+\. (?P<passed>\d+) passed; (?P<failed>\d+) failed; "
    r"(?P<ignored>\d+) ignored",
    re.MULTILINE,
)
UNITTEST_RAN = re.compile(r"^Ran (\d+) tests? in ", re.MULTILINE)
UNITTEST_SKIPPED = re.compile(r"skipped=(\d+)")
DIFF_HEADER = re.compile(r"^diff --git a/\S+ b/(?P<path>\S+)$")
COMMIT_MARKER = "COMMIT\t"


@dataclass(frozen=True)
class Verdict:
    detail: str
    problems: tuple[str, ...] = ()

    @property
    def passed(self) -> bool:
        return not self.problems


@dataclass(frozen=True)
class Check:
    gate: str
    name: str
    command: tuple[str, ...]
    evaluate: Callable[[int, str], Verdict]
    env: tuple[tuple[str, str], ...] = ()

    def line(self) -> str:
        assignments = [f"{key}={shlex.quote(value)}" for key, value in self.env]
        return " ".join([*assignments, shlex.join(self.command)])


def exit_problems(returncode: int) -> list[str]:
    return [f"exit status {returncode}"] if returncode else []


def cargo_tests(returncode: int, output: str) -> Verdict:
    """Pass when cargo exits 0, no test fails and at least one test ran."""
    passed = failed = ignored = 0
    for match in TEST_RESULT.finditer(output):
        passed += int(match["passed"])
        failed += int(match["failed"])
        ignored += int(match["ignored"])
    problems = exit_problems(returncode)
    if failed:
        problems.append(f"{failed} failed")
    if not passed:
        problems.append("no test ran")
    return Verdict(f"{passed} passed, {ignored} ignored", tuple(problems))


def unittest_tests(returncode: int, output: str) -> Verdict:
    """Pass when unittest exits 0 and ran at least one test."""
    ran = sum(int(count) for count in UNITTEST_RAN.findall(output))
    skipped = sum(int(count) for count in UNITTEST_SKIPPED.findall(output))
    problems = exit_problems(returncode)
    if not ran:
        problems.append("no test ran")
    return Verdict(f"{ran} ran, {skipped} skipped", tuple(problems))


def succeeds_with(marker: str) -> Callable[[int, str], Verdict]:
    """Pass when the command exits 0 and prints its success line."""

    def evaluate(returncode: int, output: str) -> Verdict:
        problems = exit_problems(returncode)
        found = next((line.strip() for line in output.splitlines() if marker in line), None)
        if found is None:
            problems.append(f"no {marker!r} line")
        return Verdict(found or "", tuple(problems))

    return evaluate


def diff_changes(patch: str) -> dict[str, tuple[list[str], list[str]]]:
    """Map each path in a unified diff to its (added, removed) lines."""
    changes: dict[str, tuple[list[str], list[str]]] = {}
    path = None
    in_hunk = False
    for line in patch.splitlines():
        header = DIFF_HEADER.match(line)
        if header:
            path = header["path"]
            changes.setdefault(path, ([], []))
            in_hunk = False
        elif line.startswith("@@") and path is not None:
            in_hunk = True
        elif in_hunk and path is not None and line.startswith(("+", "-")):
            added, removed = changes[path]
            (added if line.startswith("+") else removed).append(line[1:])
    return changes


def commit_patches(log: str) -> list[tuple[str, str]]:
    """Split `git log -p --format=COMMIT%x09%h%x09%s` output into (subject, patch)."""
    commits: list[tuple[str, list[str]]] = []
    for line in log.splitlines():
        if line.startswith(COMMIT_MARKER):
            commits.append((line.split("\t", 2)[-1], []))
        elif commits:
            commits[-1][1].append(line)
    return [(subject, "\n".join(lines)) for subject, lines in commits]


def codex_pins_unchanged(returncode: int, output: str) -> Verdict:
    """The pinned goldens' only commit since the base is ops-1, which added them."""
    subjects = [line.split(" ", 1)[-1] for line in output.splitlines() if line.strip()]
    problems = exit_problems(returncode)
    if subjects != [CODEX_PIN_SUBJECT]:
        problems.append(f"expected only ops-1 to touch the pins, found {subjects}")
    return Verdict("added by ops-1, unchanged since", tuple(problems))


def codex_pin_test_unchanged(returncode: int, output: str) -> Verdict:
    """After ops-1, the pin test only swaps the call that produces the tool JSON."""
    commits = commit_patches(output)
    problems = exit_problems(returncode)
    if not commits or commits[0][0] != CODEX_PIN_SUBJECT:
        problems.append(f"{CODEX_PIN_TEST} was not added by ops-1")
    for subject, patch in commits[1:]:
        added, removed = diff_changes(patch).get(CODEX_PIN_TEST, ([], []))
        if not added or [line.replace(OLD_CALL, NEW_CALL) for line in removed] != added:
            problems.append(f"{subject!r} changes more than the call expression")
    later = max(len(commits) - 1, 0)
    return Verdict(f"{later} later commit(s) swap only the call", tuple(problems))


def reviewed_app_changes(returncode: int, output: str) -> Verdict:
    """GUI sources differ from the base only by the reviewed changes."""
    changes = diff_changes(output)
    problems = exit_problems(returncode)
    for path, (added, removed) in sorted(changes.items()):
        if path in REVIEWED_APP_TESTS:
            continue
        reviewed = REVIEWED_APP_SOURCES.get(path)
        counts = (len(added), len(removed))
        if reviewed is None:
            problems.append(f"{path} has an unreviewed change")
        elif counts != reviewed:
            problems.append(f"{path} changes {counts[0]}+/{counts[1]}-, reviewed {reviewed}")
    registration = changes.get("src/app.rs")
    if registration and [line.strip() for line in registration[0]] != PARITY_REGISTRATION:
        problems.append("src/app.rs changes more than the ops parity registration")
    tests = len(changes.keys() & REVIEWED_APP_TESTS)
    detail = f"{len(changes) - tests} reviewed sources, {tests} test modules"
    return Verdict(detail, tuple(problems))


def dispatch_order(returncode: int, output: str) -> Verdict:
    """Each marker occurs once in src/main.rs, in DISPATCH_ORDER."""
    lines = []
    for line in output.splitlines():
        _, number, text = (line.split(":", 2) + ["", ""])[:3]
        if number.isdigit():
            lines.append((int(number), text))
    problems = exit_problems(returncode)
    found = []
    for marker in DISPATCH_ORDER:
        numbers = [number for number, text in lines if marker in text]
        if len(numbers) != 1:
            problems.append(f"{marker} occurs {len(numbers)} times")
        else:
            found.append(numbers[0])
    if not problems and found != sorted(found):
        problems.append(f"dispatch order changed: lines {found}")
    return Verdict("workers, --mcp/--cli, Office, engine check, GUI", tuple(problems))


def cargo_test(*arguments: str) -> tuple[str, ...]:
    return ("cargo", "test", "--locked", *arguments)


def unittest(*modules: str) -> tuple[str, ...]:
    return ("uv", "run", "--locked", "python", "-m", "unittest", *modules)


def git(*arguments: str) -> tuple[str, ...]:
    return ("git", "--no-pager", *arguments)


def local_checks(base: str = "main") -> list[Check]:
    """Every locally automatable gate check, in gate order."""
    agent = ("-p", "reshiki-agent")
    binary = ("--no-default-features", "-p", "reshiki", "--test")
    stress = "ops::headless::tests::cancelled_queued_calls_end_cancelled_while_the_rest_complete"
    audit = ("uv", "run", "--no-project", "python", "scripts/check_agent_dependencies.py")
    notices = ("tests.test_agent_dependencies", "tests.test_release", "tests.test_license_notices")
    patch = ("--no-color", "--no-ext-diff", "--unified=0", "--src-prefix=a/", "--dst-prefix=b/")
    pin_log = git("log", "-p", "--reverse", *patch, "--format=COMMIT%x09%h%x09%s", f"{base}..HEAD")
    grep = [part for marker in DISPATCH_ORDER for part in ("-e", marker)]
    links = (("RESHIKI_REQUIRE_LINK_TESTS", "1"),)
    return [
        Check(
            "G3",
            "access::escape_tests, links required",
            cargo_test(*agent, "access::escape_tests"),
            cargo_tests,
            links,
        ),
        Check("G4", "ops-6 executor", cargo_test(*agent, "ops::exec"), cargo_tests),
        Check(
            "G4",
            "ops-12(c) HeadlessHost cancellation stress",
            cargo_test(*agent, stress, "--", "--exact"),
            cargo_tests,
        ),
        Check(
            "G4",
            "reshiki-mcp framing and FakeHost tests",
            cargo_test("-p", "reshiki-mcp"),
            cargo_tests,
        ),
        Check(
            "G4",
            "safety-5 runtime invariants and heap ceiling",
            cargo_test(*binary, "agent_api_runtime"),
            cargo_tests,
        ),
        Check("G5", "malformed corpus", cargo_test(*binary, "agent_api_malformed"), cargo_tests),
        Check(
            "G6, G8",
            "dependency audit (licenses, network, serde_json)",
            audit,
            succeeds_with("Agent dependencies passed"),
        ),
        Check(
            "G6", "audit, release notices() and notice tests", unittest(*notices), unittest_tests
        ),
        Check(
            "G6",
            "license consolidation",
            ("uv", "run", "--no-project", "python", "-c", LICENSE_CHECK),
            succeeds_with("Consolidated notices:"),
        ),
        Check("G7", "experimental labels", cargo_test(*binary, "agent_api_labels"), cargo_tests),
        Check("G7", "filesystem fence", unittest("tests.test_agent_fs_fence"), unittest_tests),
        Check(
            "G7",
            "documentation build and links",
            ("bun", "run", "docs:build"),
            succeeds_with("local documentation links"),
        ),
        Check(
            "G8", "ops-1 Codex byte pins", cargo_test(*binary, "assistant_contract"), cargo_tests
        ),
        Check(
            "G8",
            "Codex pins unchanged after ops-1",
            git("log", "--format=%h %s", f"{base}..HEAD", "--", *CODEX_PINS),
            codex_pins_unchanged,
        ),
        Check(
            "G8",
            "pin test changed only by ops-3's call",
            (*pin_log, "--", CODEX_PIN_TEST),
            codex_pin_test_unchanged,
        ),
        Check(
            "G8",
            "GUI sources: reviewed changes only",
            git("diff", *patch, f"{base}...HEAD", "--", *APP_PATHS),
            reviewed_app_changes,
        ),
        Check(
            "G8",
            "src/main.rs dispatch order",
            git("grep", "-n", "--no-color", "-F", *grep, "--", "src/main.rs"),
            dispatch_order,
        ),
    ]


def run_check(check: Check, root: Path = ROOT) -> tuple[Verdict, str]:
    completed = subprocess.run(
        list(check.command),
        cwd=root,
        env={**os.environ, **dict(check.env)},
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        errors="replace",
    )
    return check.evaluate(completed.returncode, completed.stdout), completed.stdout


def table(rows: Sequence[Sequence[str]]) -> str:
    widths = [max(len(row[column]) for row in rows) for column in range(len(rows[0]))]
    return "\n".join(
        "  ".join(cell.ljust(width) for cell, width in zip(row, widths)).rstrip() for row in rows
    )


def pending_report() -> str:
    rows = [("Gate", "Pending evidence", "Source"), *PENDING]
    return f"Pending evidence, not checked here (record it in {RECORD}):\n{table(rows)}"


def run_local(checks: list[Check], root: Path = ROOT) -> bool:
    rows = [("Gate", "Check", "Result", "Detail")]
    for check in checks:
        print(f"[{check.gate}] {check.name}: {check.line()}", flush=True)
        verdict, output = run_check(check, root)
        if not verdict.passed:
            print("\n".join(output.splitlines()[-OUTPUT_TAIL:]), flush=True)
        detail = "; ".join(verdict.problems) or verdict.detail
        rows.append((check.gate, check.name, "pass" if verdict.passed else "FAIL", detail))
    print(f"\n{table(rows)}\n")
    print("Commands:")
    for check in checks:
        print(f"  [{check.gate}] {check.line()}")
    print(f"\n{pending_report()}")
    return all(row[2] == "pass" for row in rows[1:])


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--local", action="store_true", help="run the local checks")
    parser.add_argument("--base", default="main", help="the branch the G8 diffs compare with")
    args = parser.parse_args(argv)
    checks = local_checks(args.base)
    if not args.local:
        print("Local checks (run them with --local):")
        for check in checks:
            print(f"  [{check.gate}] {check.name}: {check.line()}")
        print(f"\n{pending_report()}")
        return 0
    return 0 if run_local(checks) else 1


if __name__ == "__main__":
    sys.exit(main())
