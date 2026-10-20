I found one correctness issue and one documentation mismatch.

1. **[P2] Selecting Automatic after a ChemDraw round trip can export the previous flush alignment.**  
   At [crates/io/src/exchange/drawing/labels.rs:581](/Users/shiranui/dev/ReShiki/.claude/worktrees/agent-a826e1a78594af95e/crates/io/src/exchange/drawing/labels.rs:581), a free-text label without an explicit attachment falls back to its stored hydrogen position for `LabelDisplay`.

   Copy a **Flush Left MeO** label to ChemDraw and paste it back. The importer stores `LabelDisplay="Left"` as `HydrogenPosition::Left`, while separately restoring `LabelAttachment::Left`. Selecting **Bond attachment → Automatic** changes only `label_attachment`. The next copy consequently emits `LabelDisplay="Left"` again, so ChemDraw retains Flush Left despite the Automatic selection. Right behaves the same way.

   The new export behavior introduces this stale hydrogen setting through successful alignment round trips. Free-text Auto/Character fallback should be independent of hydrogen position, or import should avoid treating free-text alignment as a hydrogen setting. The current test starts each case with a fresh atom whose hydrogen position is Auto, so it misses this sequence.

2. **[P3] “Leave Automatic untagged” does not describe free-text exports.**  
   [docs/chemdraw-attachment-comparison.md:23](/Users/shiranui/dev/ReShiki/.claude/worktrees/agent-a826e1a78594af95e/docs/chemdraw-attachment-comparison.md:23) says exports leave Automatic untagged. That is true for group wrappers, but free-text nodes still receive `LabelDisplay="Auto"` from `labels()`—or a stored hydrogen-side value as described above. The updated unit test explicitly expects the Auto attribute. Qualify the statement to distinguish group wrappers from free-text nodes.

I found no additional concrete issues in the generic, hidden, textless, strict file-export, or ordinary hydrogen-position paths. The four fixtures match the supplied ChemDraw observations; centered attachment is restored separately from the `Center → HydrogenPosition::Auto` mapping.

**Validation:** 18 focused tests passed using the existing test binaries and InChI helper. `git diff --check` passed. No files were edited or rebuilt.