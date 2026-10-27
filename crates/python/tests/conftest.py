from pathlib import Path

import pytest


@pytest.fixture(scope="session")
def fixtures() -> Path:
    return Path(__file__).resolve().parents[3] / "tests" / "fixtures"
