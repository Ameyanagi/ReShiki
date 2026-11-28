"""The LGPL-3.0-or-later axtb engine used by :mod:`reshiki.xtb`.

The matching ReShiki release provides the complete source archive at
https://github.com/Ameyanagi/ReShiki/releases. The axtb fork is at
https://github.com/Ameyanagi/axtb.
"""

import os
import sys
from importlib.metadata import distribution, version
from pathlib import Path

__version__ = version("reshiki-xtb")


def worker_path() -> Path:
    """Return the installed axtb worker executable, using this wheel's file record.

    Raise ``FileNotFoundError`` if the installation needs repair.
    """
    installed = distribution("reshiki-xtb")
    executable = "reshiki-xtb-worker" + (".exe" if sys.platform == "win32" else "")
    for file in installed.files or ():
        if file.name == executable:
            path = Path(str(installed.locate_file(file))).resolve()
            if path.is_file() and os.access(path, os.X_OK):
                return path
    raise FileNotFoundError(
        "The installed xTB worker is missing or cannot run. "
        "To repair it, reinstall with: pip install --force-reinstall reshiki-xtb"
    )
