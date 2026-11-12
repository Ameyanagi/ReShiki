"""SUP groups of a MOL export on stdin, as RDKit reads them (#870)."""

import json
import sys

from molfile_engine_reference import superatoms
from rdkit import RDLogger

if __name__ == "__main__":
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(superatoms(sys.stdin.read()) or []))
