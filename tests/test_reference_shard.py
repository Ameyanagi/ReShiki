"""Every workspace integration test has a reference shard before it merges.

The live reference run is weekly or manual, so an unassigned target used to
break it only after merging. This check runs with the regular Python tests.
"""

import json
import shutil
import subprocess
import unittest

from scripts.run_reference_shard import (
    DEFAULT_SECONDS,
    METADATA_COMMAND,
    ROOT,
    SHARD_SECONDS,
    TARGET_SECONDS,
    assign_targets,
    integration_targets,
)


class AssignmentTests(unittest.TestCase):
    def test_every_target_runs_once_and_shard_0_leaves_room_for_its_workspace_tests(self):
        targets = sorted([*TARGET_SECONDS, *(f"quick_{index}" for index in range(40))])
        shards = assign_targets(targets, 4)
        self.assertEqual(sorted(name for shard in shards for name in shard), targets)
        loads = [
            sum(TARGET_SECONDS.get(name, DEFAULT_SECONDS) for name in shard) for shard in shards
        ]
        self.assertLess(loads[0] + SHARD_SECONDS[0] / 2, min(loads[1:]))


@unittest.skipUnless(shutil.which("cargo"), "Cargo is required to read workspace targets")
class ReferenceShardTests(unittest.TestCase):
    def metadata(self):
        result = subprocess.run(
            METADATA_COMMAND, cwd=ROOT, capture_output=True, text=True, check=True
        )
        return json.loads(result.stdout)

    def test_every_workspace_integration_target_has_a_shard(self):
        targets = integration_targets(self.metadata())
        # Shard 0's fixed workspace load must still leave it integration targets.
        self.assertTrue(all(assign_targets(targets, 4)))

    def test_an_unassigned_target_is_named(self):
        metadata = self.metadata()
        package = next(
            p
            for p in metadata["packages"]
            if p["id"] in metadata["workspace_members"] and p["name"] != "reshiki"
        )
        package["targets"].append({"name": "new_suite", "kind": ["test"]})
        with self.assertRaisesRegex(ValueError, f"{package['name']}::new_suite"):
            integration_targets(metadata)


if __name__ == "__main__":
    unittest.main()
