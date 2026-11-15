"""The error ReShiki raises, shared by the Python layer and the Rust extension."""

from __future__ import annotations


class ReShikiError(ValueError):
    """A ReShiki operation failed; ``code`` says why, as the MCP tools and the CLI report it.

    ``invalid_arguments``: a format, option or style is not accepted.
    ``failed``: the structure could not be read, drawn or written.

    It is a ``ValueError``, so existing ``except ValueError`` handlers keep working.
    """

    code: str

    def __init__(self, message: str, code: str = "failed") -> None:
        super().__init__(message)
        self.code = code
