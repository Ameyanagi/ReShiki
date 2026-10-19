"""Every workspace integration test has a reference shard before it merges.

The live reference run is weekly or manual, so an unassigned target used to
break it only after merging. This check runs with the regular Python tests.
"""

import json
import shutil
import subprocess
import unittest

from scripts.run_reference_shard import METADATA_COMMAND, ROOT, integration_targets


@unittest.skipUnless(shutil.which("cargo"), "Cargo is required to read workspace targets")
class ReferenceShardTests(unittest.TestCase):
    def metadata(self):
        result = subprocess.run(
            METADATA_COMMAND, cwd=ROOT, capture_output=True, text=True, check=True
        )
        return json.loads(result.stdout)

    def test_every_workspace_integration_target_has_a_shard(self):
        self.assertTrue(integration_targets(self.metadata()))

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
