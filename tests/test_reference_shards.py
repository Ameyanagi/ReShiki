"""Reference shards keep every Cargo integration target and propagate failures."""

import contextlib
import copy
import io
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import run_reference_shard as runner


def metadata(names):
    return {
        "workspace_members": ["reshiki-id", "helper-id"],
        "packages": [
            {
                "name": "reshiki",
                "id": "reshiki-id",
                "targets": [
                    {"name": "reshiki", "kind": ["lib"]},
                    {"name": "reshiki", "kind": ["bin"]},
                    {"name": "demo", "kind": ["example"]},
                    {"name": "build-script-build", "kind": ["custom-build"]},
                    *[{"name": name, "kind": ["test"]} for name in names],
                ],
            },
            {"name": "helper", "id": "helper-id", "targets": []},
        ],
    }


class ReferenceShardTests(unittest.TestCase):
    def test_metadata_and_balancing_include_every_target_once_in_a_stable_assignment(self):
        names = list(runner.TARGET_SECONDS) + [f"ordinary_{i}" for i in range(100)]
        data = metadata(names)
        data["packages"][0]["targets"][-1]["required-features"] = ["rdkit-reference"]
        targets = runner.integration_targets(data)
        shards = runner.assign_targets(targets, 4)
        self.assertEqual(sorted(name for shard in shards for name in shard), sorted(names))
        self.assertTrue(all(shards))
        for index, shard in enumerate(shards):
            for other in shards[index + 1 :]:
                self.assertFalse(set(shard) & set(other))
        self.assertEqual(runner.assign_targets(list(reversed(targets)), 4), shards)
        loads = [
            sum(runner.TARGET_SECONDS.get(name, runner.DEFAULT_SECONDS) for name in shard)
            for shard in shards
        ]
        self.assertLessEqual(max(loads) - min(loads), runner.DEFAULT_SECONDS)
        heavy_shards = [
            next(i for i, shard in enumerate(shards) if name in shard)
            for name in [
                "native_aromatic",
                "inchi_generator",
                "engine_migration",
                "reaction_smiles",
            ]
        ]
        self.assertEqual(len(set(heavy_shards)), 4)
        # An auto-discovered target has no required-features metadata. It must
        # be selected without updating the timing table or runner source.
        data["packages"][0]["targets"].append({"name": "new_implicit_test", "kind": ["test"]})
        updated = runner.assign_targets(runner.integration_targets(data), 4)
        self.assertEqual(sum(shard.count("new_implicit_test") for shard in updated), 1)

    def test_malformed_metadata_fails_instead_of_silently_dropping_tests(self):
        valid = metadata(["one", "two", "three", "four"])
        malformed = [None, {}, {"packages": [], "workspace_members": []}]
        for field, value in [("targets", None), ("id", "not-a-member")]:
            changed = copy.deepcopy(valid)
            changed["packages"][0][field] = value
            malformed.append(changed)
        for target in [None, {"kind": "test"}, {"kind": ["test"]}, {"kind": ["test"], "name": ""}]:
            changed = copy.deepcopy(valid)
            changed["packages"][0]["targets"].append(target)
            malformed.append(changed)
        malformed.extend([metadata([]), metadata(["duplicate", "duplicate"])])
        changed = copy.deepcopy(valid)
        changed["packages"].append(copy.deepcopy(changed["packages"][0]))
        malformed.append(changed)
        changed = copy.deepcopy(valid)
        changed["packages"][1]["targets"] = [{"name": "new_helper_integration", "kind": ["test"]}]
        malformed.append(changed)
        for data in malformed:
            with self.subTest(data=data), self.assertRaises(ValueError):
                runner.integration_targets(data)

    def test_invalid_indexes_and_empty_shards_fail_before_running_tests(self):
        with patch.object(runner.subprocess, "run") as cargo:
            for index, count in [(-1, 4), (4, 4), (0, 0), (0, -1)]:
                with self.subTest(index=index, count=count), self.assertRaises(ValueError):
                    runner.run_shard(index, count)
            cargo.assert_not_called()
        for targets, count in [([], 4), (["one"], 4), (["one", "one"], 2), ([""], 1)]:
            with self.subTest(targets=targets, count=count), self.assertRaises(ValueError):
                runner.assign_targets(targets, count)
        with self.assertRaises(ValueError):
            runner.commands_for([], 0)

    def test_batches_preserve_reference_flags_and_run_workspace_units_and_docs_once(self):
        shards = runner.assign_targets(["alpha", "beta", "gamma", "delta", "epsilon"], 4)
        all_commands = []
        for index, selected in enumerate(shards):
            commands = runner.commands_for(selected, index)
            self.assertEqual(len(commands), 3 if index == 0 else 1)
            integration = commands[0]
            self.assertEqual(integration.count("--test"), len(selected))
            actual = [integration[i + 1] for i, arg in enumerate(integration) if arg == "--test"]
            self.assertEqual(actual, selected)
            self.assertEqual(integration[integration.index("--package") + 1], "reshiki")
            for command in commands:
                self.assertEqual(command[:2], ["cargo", "test"])
                self.assertIn("--locked", command)
                self.assertIn("--no-fail-fast", command)
                self.assertEqual(command[command.index("--features") + 1], "rdkit-reference")
                self.assertNotIn("--", command)
            all_commands.extend(commands)
        workspace = [command for command in all_commands if "--workspace" in command]
        self.assertEqual(len(workspace), 2)
        self.assertIn("--lib", workspace[0])
        self.assertIn("--bins", workspace[0])
        self.assertIn("--doc", workspace[1])

    def test_failed_batch_is_reported_and_later_batches_still_execute(self):
        for results, expected in [([101, 0, 0], 101), ([0, 3, 7], 3), ([0, 0, 0], 0)]:
            with self.subTest(results=results), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                responses = [
                    subprocess.CompletedProcess(
                        [], 0, json.dumps(metadata(["a", "b", "c", "d"])), ""
                    ),
                    *[subprocess.CompletedProcess([], code) for code in results],
                ]
                with patch.object(runner.subprocess, "run", side_effect=responses) as cargo:
                    with contextlib.redirect_stdout(io.StringIO()):
                        self.assertEqual(runner.run_shard(0, 4, root), expected)
                self.assertEqual(cargo.call_count, 4)
                manifest = json.loads((root / "artifacts/reference-shard-0.json").read_text())
                self.assertEqual(manifest["returncode"], expected)
                self.assertEqual([batch["returncode"] for batch in manifest["commands"]], results)
                self.assertTrue(
                    all(batch["status"] in ["passed", "failed"] for batch in manifest["commands"])
                )
                self.assertEqual(manifest["total_targets"], 4)
                self.assertEqual(manifest["targets"], ["a"])
                for call, batch in zip(cargo.call_args_list[1:], manifest["commands"]):
                    self.assertEqual(call.args[0], batch["command"])
                    self.assertEqual(call.kwargs, {"cwd": root, "check": False})

    def test_bad_metadata_and_missing_cargo_are_recorded_without_running_tests(self):
        for result, expected in [
            (subprocess.CompletedProcess([], 101, "", "metadata failed"), 101),
            (subprocess.CompletedProcess([], 0, "not JSON", ""), 2),
            (subprocess.CompletedProcess([], 0, json.dumps(metadata(["only-one"])), ""), 2),
            (FileNotFoundError("cargo missing"), 2),
        ]:
            with self.subTest(result=result), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                with patch.object(runner.subprocess, "run", side_effect=[result]) as cargo:
                    with contextlib.redirect_stderr(io.StringIO()):
                        self.assertEqual(runner.run_shard(1, 4, root), expected)
                cargo.assert_called_once_with(
                    runner.METADATA_COMMAND, cwd=root, capture_output=True, text=True, check=False
                )
                manifest = json.loads((root / "artifacts/reference-shard-1.json").read_text())
                self.assertEqual(manifest["returncode"], expected)
                self.assertTrue(manifest["error"])
                self.assertEqual(manifest["commands"], [])

    def test_cargo_launch_error_still_runs_later_batches(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            responses = [
                subprocess.CompletedProcess([], 0, json.dumps(metadata(["a", "b", "c", "d"])), ""),
                OSError("cannot launch cargo"),
                subprocess.CompletedProcess([], 0),
                subprocess.CompletedProcess([], 0),
            ]
            with patch.object(runner.subprocess, "run", side_effect=responses) as cargo:
                with (
                    contextlib.redirect_stdout(io.StringIO()),
                    contextlib.redirect_stderr(io.StringIO()),
                ):
                    self.assertEqual(runner.run_shard(0, 4, root), 127)
            self.assertEqual(cargo.call_count, 4)
            manifest = json.loads((root / "artifacts/reference-shard-0.json").read_text())
            self.assertEqual(manifest["commands"][0]["error"], "cannot launch cargo")
            self.assertEqual(manifest["commands"][-1]["status"], "passed")


if __name__ == "__main__":
    unittest.main()
