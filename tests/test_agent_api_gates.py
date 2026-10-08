"""The agent API gate runner's command assembly and result parsing, with no subprocess."""

import contextlib
import io
import shlex
import subprocess
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import agent_api_gates as gates

GATES = ("G1", "G2", "G3", "G4", "G5", "G6", "G7", "G8")
OPS_3 = "refactor(agent): split canvas tool definitions into a neutral ToolSpec and a Codex adapter"
CARGO = "running 3 tests\ntest result: ok. 3 passed; 0 failed; 1 ignored; 0 measured\n"
OLD_BLOB, NEW_BLOB = "0" * 40, "1" * 40


def diff(files, blobs=None):
    """A unified diff with one hunk per path, as `git diff --full-index --unified=0` prints it."""
    lines = []
    for path, (added, removed) in files.items():
        blob = (blobs or {}).get(path, NEW_BLOB)
        lines += [f"diff --git a/{path} b/{path}", f"index {OLD_BLOB}..{blob} 100644"]
        lines += [f"--- a/{path}", f"+++ b/{path}", "@@ -1 +1 @@"]
        lines += [f"-{line}" for line in removed] + [f"+{line}" for line in added]
    return "\n".join(lines) + "\n"


def reviewed_app(**blobs):
    """The stack's GUI diff, with `blobs` as the new blob ids of some paths."""
    paths = [*gates.REVIEWED_APP_SOURCES, *sorted(gates.REVIEWED_APP_TESTS), *blobs]
    files = {path: (["changed"], ["changed"]) for path in paths}
    return diff(files, {**gates.REVIEWED_APP_SOURCES, **blobs})


def contract_log(*later):
    """`git log -p --reverse` of the pin test: ops-1 adds it, then each later commit."""
    test = gates.CODEX_PIN_TEST
    created = diff({test: ([f"    {gates.OLD_CALL}.to_string(),", "fn golden() {}"], [])})
    commits = [f"COMMIT\td8314670\t{gates.CODEX_PIN_SUBJECT}\n\n{created}"]
    for index, (subject, removed, added) in enumerate(later):
        commits.append(f"COMMIT\t{index:07x}\t{subject}\n\n{diff({test: (added, removed)})}")
    return "".join(commits)


SWAP = (OPS_3, [f"    {gates.OLD_CALL}.to_string(),"], [f"    {gates.NEW_CALL}.to_string(),"])


def dispatch(order=gates.DISPATCH_ORDER):
    return "".join(
        f"src/main.rs:{number}:    if {marker} {{\n"
        for number, marker in enumerate(order, start=36)
    )


def answer(command, failing=()):
    """What each local check's command prints when it succeeds."""
    if command[0] == "cargo":
        output = CARGO
    elif "unittest" in command:
        output = "....\n----\nRan 4 tests in 0.2s\n\nOK (skipped=1)\n"
    elif "scripts/check_agent_dependencies.py" in command:
        output = "Agent dependencies passed for 6 release targets.\n"
    elif "-c" in command:
        output = "Consolidated notices: 2048 bytes\n"
    elif command[0] == "bun":
        output = "Verified 7816 local documentation links and assets.\n"
    elif command[2:4] == ["log", "-p"]:
        output = contract_log(SWAP)
    elif command[2] == "log":
        output = f"d8314670 {gates.CODEX_PIN_SUBJECT}\n"
    elif command[2] == "diff":
        output = reviewed_app()
    else:
        output = dispatch()
    code = 101 if any(part in command for part in failing) else 0
    return subprocess.CompletedProcess(command, code, stdout=output)


def run_main(*argv, failing=()):
    """main(argv) with a fake subprocess.run; returns (exit code, stdout, calls)."""
    calls = []

    def run(command, **options):
        calls.append((command, options))
        return answer(command, failing)

    out = io.StringIO()
    with patch("agent_api_gates.subprocess.run", side_effect=run), contextlib.redirect_stdout(out):
        code = gates.main(list(argv))
    return code, out.getvalue(), calls


class CommandTests(unittest.TestCase):
    def test_checks_follow_gate_order_from_g3_to_g8(self):
        order = [check.gate[:2] for check in gates.local_checks()]
        self.assertEqual(order, sorted(order))
        self.assertEqual(set(order), set(GATES[2:]))

    def test_the_escape_suite_requires_link_tests(self):
        check = gates.local_checks()[0]
        self.assertEqual(
            check.line(),
            "RESHIKI_REQUIRE_LINK_TESTS=1 cargo test --locked -p reshiki-agent access::escape_tests",
        )

    def test_binary_tests_use_the_ci_feature_set(self):
        tests = {
            "agent_api_runtime",
            "agent_api_malformed",
            "agent_api_labels",
            "assistant_contract",
        }
        commands = [c.command for c in gates.local_checks() if c.command[-1] in tests]
        self.assertEqual({command[-1] for command in commands}, tests)
        for command in commands:
            self.assertEqual(
                command[:-1],
                ("cargo", "test", "--locked", "--no-default-features", "-p", "reshiki", "--test"),
            )

    def test_the_base_reaches_every_diff_check(self):
        for base, checks in (
            (gates.BASELINE_COMMIT, gates.local_checks()),
            ("origin/main", gates.local_checks("origin/main")),
        ):
            for check in checks:
                if check.command[0] == "git" and "grep" not in check.command:
                    self.assertTrue({f"{base}..HEAD", f"{base}...HEAD"} & set(check.command))

    def test_the_default_base_is_main_before_p1_so_it_survives_landing(self):
        self.assertEqual(gates.BASELINE_COMMIT, "40c8d82a5ed213685a7eedd150a0b7adf6899846")

    def test_printed_command_lines_round_trip(self):
        for check in gates.local_checks():
            words = shlex.split(check.line())
            self.assertEqual(words[len(check.env) :], list(check.command), check.name)

    def test_every_gate_keeps_ci_or_manual_evidence_pending(self):
        self.assertEqual({gate for gate, _, _ in gates.PENDING}, set(GATES))
        self.assertFalse({check.gate for check in gates.local_checks()} & {"G1", "G2"})


class ParsingTests(unittest.TestCase):
    def test_cargo_counts_every_test_binary(self):
        verdict = gates.cargo_tests(0, CARGO + CARGO.replace("3 passed", "10 passed"))
        self.assertTrue(verdict.passed)
        self.assertEqual(verdict.detail, "13 passed, 2 ignored")

    def test_a_filter_that_matches_nothing_fails(self):
        output = "test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 30 filtered out\n"
        self.assertEqual(gates.cargo_tests(0, output).problems, ("no test ran",))

    def test_failed_tests_and_exit_status_fail(self):
        output = "test result: FAILED. 3 passed; 1 failed; 0 ignored; 0 measured\n"
        self.assertEqual(gates.cargo_tests(101, output).problems, ("exit status 101", "1 failed"))
        self.assertEqual(gates.cargo_tests(101, "error[E0425]: oops\n").problems[-1], "no test ran")

    def test_unittest_needs_a_test_and_a_clean_exit(self):
        verdict = gates.unittest_tests(0, "Ran 5 tests in 0.1s\n\nOK (skipped=2)\n")
        self.assertEqual((verdict.passed, verdict.detail), (True, "5 ran, 2 skipped"))
        self.assertEqual(
            gates.unittest_tests(0, "Ran 0 tests in 0.0s\n\nOK\n").problems, ("no test ran",)
        )
        self.assertFalse(gates.unittest_tests(1, "Ran 5 tests in 0.1s\n\nFAILED\n").passed)

    def test_commands_must_exit_0_and_print_their_success_line(self):
        evaluate = gates.succeeds_with("Agent dependencies passed")
        verdict = evaluate(0, "noise\nAgent dependencies passed for 6 release targets.\n")
        self.assertEqual(verdict.detail, "Agent dependencies passed for 6 release targets.")
        self.assertFalse(evaluate(0, "nothing\n").passed)
        self.assertFalse(evaluate(1, "Agent dependencies passed\n").passed)

    def test_only_ops_1_may_touch_the_codex_pins(self):
        ops_1 = f"d8314670 {gates.CODEX_PIN_SUBJECT}\n"
        self.assertTrue(gates.codex_pins_unchanged(0, ops_1).passed)
        self.assertFalse(gates.codex_pins_unchanged(0, "").passed)
        self.assertFalse(gates.codex_pins_unchanged(0, f"0123456 regenerate pins\n{ops_1}").passed)

    def test_the_pin_test_may_only_swap_the_call_expression(self):
        self.assertTrue(gates.codex_pin_test_unchanged(0, contract_log(SWAP)).passed)
        self.assertTrue(gates.codex_pin_test_unchanged(0, contract_log()).passed)
        assertion = (OPS_3, ["    assert_eq!(a, b);"], ["    assert_ne!(a, b);"])
        self.assertFalse(gates.codex_pin_test_unchanged(0, contract_log(SWAP, assertion)).passed)
        added_only = ("extra", [], ["    assert!(true);"])
        self.assertFalse(gates.codex_pin_test_unchanged(0, contract_log(added_only)).passed)
        not_ops_1 = contract_log(SWAP).replace(gates.CODEX_PIN_SUBJECT, "test: other")
        self.assertFalse(gates.codex_pin_test_unchanged(0, not_ops_1).passed)

    def test_gui_sources_allow_only_the_reviewed_changes(self):
        verdict = gates.reviewed_app_changes(0, reviewed_app())
        self.assertTrue(verdict.passed, verdict.problems)
        self.assertEqual(verdict.detail, "4 reviewed sources, 3 test modules")
        self.assertTrue(gates.reviewed_app_changes(0, "").passed)
        canvas = gates.reviewed_app_changes(0, reviewed_app(**{"src/canvas/tools.rs": NEW_BLOB}))
        self.assertEqual(canvas.problems, ("src/canvas/tools.rs has an unreviewed change",))
        other = reviewed_app(**{"src/app.rs": NEW_BLOB})
        self.assertEqual(
            gates.reviewed_app_changes(0, other).problems,
            ("src/app.rs is not its reviewed version c4dcaa29",),
        )

    def test_an_edit_that_keeps_the_reviewed_line_counts_still_fails(self):
        # ops-4 adds 1 line to src/app/files.rs and removes 3; so does this edit.
        removed = ["let text = read(&path)?;", "let document =", "    from_native_file(&text)?;"]
        files = {"src/app/files.rs": (["Document::from_json(&read(&path)?)?"], removed)}
        edited = diff(files, {"src/app/files.rs": NEW_BLOB})
        self.assertEqual(
            gates.reviewed_app_changes(0, edited).problems,
            ("src/app/files.rs is not its reviewed version fbd83770",),
        )

    def test_a_change_without_a_new_blob_is_not_the_reviewed_version(self):
        mode_only = "diff --git a/src/app.rs b/src/app.rs\nold mode 100644\nnew mode 100755\n"
        self.assertEqual(gates.diff_blobs(mode_only), {"src/app.rs": None})
        self.assertFalse(gates.reviewed_app_changes(0, mode_only).passed)

    def test_dispatch_order_is_pinned(self):
        self.assertTrue(gates.dispatch_order(0, dispatch()).passed)
        moved = list(gates.DISPATCH_ORDER)
        moved.insert(moved.index("enable_office_embedding()"), moved.pop(3))
        self.assertIn(
            "dispatch order changed", gates.dispatch_order(0, dispatch(moved)).problems[0]
        )
        missing = dispatch(gates.DISPATCH_ORDER[:-1])
        self.assertEqual(
            gates.dispatch_order(1, missing).problems,
            ("exit status 1", "iced::application( occurs 0 times"),
        )


class RunTests(unittest.TestCase):
    def test_local_runs_every_check_in_the_checkout_and_reports_pending_evidence(self):
        code, out, calls = run_main("--local")
        self.assertEqual(code, 0, out)
        checks = gates.local_checks()
        self.assertEqual([command for command, _ in calls], [list(c.command) for c in checks])
        for _, options in calls:
            self.assertEqual(options["cwd"], gates.ROOT)
        self.assertEqual(calls[0][1]["env"]["RESHIKI_REQUIRE_LINK_TESTS"], "1")
        self.assertEqual(out.count("  pass  "), len(checks))
        self.assertNotIn("FAIL", out)
        for check in checks:
            self.assertIn(f"  [{check.gate}] {check.line()}\n", out)
        pending = out[out.index("Pending evidence") :]
        for _, evidence, source in gates.PENDING:
            self.assertIn(evidence, pending)
            self.assertIn(source, pending)

    def test_a_failing_check_fails_the_run_and_shows_its_output(self):
        code, out, _ = run_main("--local", failing=("agent_api_malformed",))
        self.assertEqual(code, 1)
        lines = out.splitlines()
        rows = [line.split() for line in lines if "FAIL" in line]
        self.assertEqual(rows, [["G5", "malformed", "corpus", "FAIL", "exit", "status", "101"]])
        started = lines.index(next(line for line in lines if line.startswith("[G5] malformed")))
        self.assertEqual(lines[started + 1 : started + 3], CARGO.splitlines())

    def test_without_local_nothing_runs(self):
        code, out, calls = run_main()
        self.assertEqual((code, calls), (0, []))
        self.assertIn("run them with --local", out)
        self.assertIn("Pending evidence", out)


if __name__ == "__main__":
    unittest.main()
