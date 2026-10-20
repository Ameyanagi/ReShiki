Wrote [E/report.md](/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev378/final/before/report.md).

**Both round trips were blocked before ChemDraw paste.** Repeated computer-use bindings to `/Applications/ChemDraw.app` timed out while its existing process consumed about 97–98% CPU. No step 6 values were obtained, so the expected regression was neither confirmed nor disproved.

Saved both source screenshots and clipboard captures:

| Case | Source bond attachment | Source hydrogen position | Round-trip readback |
|---|---|---|---|
| L | Flush left | Automatic | Not obtained |
| C | Centered | Automatic | Not obtained |

The driven ReShiki PID was **47639**, verified against the requested binary using its executable path, shared hard-link inode, and SHA-256. That process exited, and the temporary wrapper was removed.

ChemDraw’s existing process and documents were left untouched. No repository files or system settings were changed.