# Optional object toolbar (#66)

Turn on **View → Object toolbar** for direct access to front/back, edge and
center alignment, horizontal/vertical distribution, horizontal/vertical
reflection, and 180° rotation. Hide it with its × control or the same View
checkbox. The preference survives restarting ReShiki and does not alter the
drawing file.

The toolbar keeps its height when selection changes. Each icon has a tooltip;
alignment becomes available for two independent objects and distribution for
three. Connected selected atoms and persistent groups count as single objects.
At narrower canvas widths the command strip scrolls horizontally while the
hide control stays accessible. The existing inspector controls remain available.

Front/back uses ReShiki's existing graphics and bond-depth ordering. A mixed
selection updates both in one Undo step. Text and reaction arrows do not have
editable stacking order; selecting only those objects disables front/back.
Reflection preserves stereochemistry through the existing transform command.
No shortcuts were added.

![Selected molecules with the optional object toolbar visible](../images/object-toolbar/selected.png)

![The toolbar preference and all controls fit in a 1040-pixel window](../images/object-toolbar/compact.png)

## Reproduce the example

The opt-in renderer check creates `artifacts/object-toolbar-qa/objects.rsk`:
three separate two-atom molecules at different positions. Open this file, choose
**View → Object toolbar**, close View, and select all objects. Try **Align top
edges**, **Distribute horizontally**, **Flip horizontal**, and **Rotate selection
180°**, undoing each operation. Click empty canvas to confirm that the strip
stays in place with its commands disabled. Toggle the toolbar off and on, and
restart to check the preference.

Base: `0ae0fb6a21c5e5c5b90ae66730458fda4ea2a20d` (`origin/main` when branched).
Head: the change containing this document. Capture platform: macOS 26.5.1,
Apple Silicon; the Iced application renderer, PNG at 1× output scale, fitted
canvas (234% at 1280×820; 182% at 1040×680). The renderer test records the fixture and
exact window conditions. Headless evidence supplements the desktop interaction
check; it does not claim to be a desktop screenshot.

## Validation

All six focused tests, including the opt-in actual-renderer and pointer test,
passed. All four generated light/dark/selection/layout images were inspected.
Real desktop interaction is pending central integration QA; this PR remains a
draft until that check is recorded.

- Toolbar command availability for zero, one, two, and three molecular objects.
- Mixed graphics/bond layering changes one history step, with Undo/Redo and native
  JSON save/reopen checks; individual graphic and bond commands keep their scope.
- Alignment, distribution, both reflections, and 180° rotation dispatch through
  standard commands and Undo/Redo.
- Preference serialization preserves old appearance settings and does not modify
  document history.
- Opt-in real renderer snapshots in light/dark and compact layouts; pointer
  events on the hide control, with fixed toolbar height in all selection states.

```sh
cargo test --locked --bin reshiki object_toolbar::tests
cargo test --locked --bin reshiki object_toolbar_headless_snapshot -- --ignored --nocapture
```

Release caption: **Show the optional object toolbar for one-click alignment,
distribution, reflection, rotation, and graphics/bond stacking.**
