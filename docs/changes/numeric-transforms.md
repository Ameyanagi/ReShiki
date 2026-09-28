# Precise numeric transforms

Issue: [#65](https://github.com/Ameyanagi/ReShiki/issues/65).

Select objects, open **Properties → Arrange & transform**, and enter a value
under **Precise transforms**. Each **Apply** button, or Enter in its input,
applies just that field as one Undo step. Typing alone does not change the
drawing. Changing the selection, undoing, or applying an edit refreshes the
displayed values.

- **Rotate °** is a relative angle; positive values rotate clockwise. For
  example, enter `72` for a pentagon-sized turn.
- **Tilt X ° / Tilt Y °** use the existing orthographic projection, with an
  allowed change of −85° through 85°. As with the existing tilt controls,
  atoms and shapes tilt; arrows and upright captions do not.
- **Width pt / Height pt** specify the canvas selection bounds in publication
  points, including labels. **Lock proportions for width / height** scales
  coordinates uniformly; clear it to change one axis independently.
- **Scale %** is relative uniform coordinate scaling: `125` enlarges by 25%.
  It resets to `100` after applying; angles reset to `0`.

Font sizes, line widths, and arrowhead styling stay fixed. Their contribution
to the selection bounds means that coordinate scaling and the ratio of the
displayed dimensions can differ. Width and height are solved against the same
bounds used by the selection box; impossible sizes, such as enlarging a lone
caption without changing its font, show an error without modifying history.
Unchanged displayed dimensions, full turns, zero tilt, and 100% scale are
no-ops. Groups, hidden abbreviation atoms, and native document data continue
through the existing transform APIs. Partial molecular selections retain the
existing boundary stereochemistry invalidation behavior.

## Evidence and reproduction

Base: `0ae0fb6a21c5e5c5b90ae66730458fda4ea2a20d` (main). Head: the implementation
commit containing this file. Platform: macOS arm64, Rust debug build, native
chemistry engine with the packaged InChI helper. This is a new feature, so the
image shows the new controls rather than a matched bug comparison.

![Numeric rotation, tilt, physical dimensions, percentage scale, and proportion lock](../images/pr-reviews/numeric-transforms-panel.png)

The [regression fixture and renderer check](../../src/app/numeric_transforms.rs)
generate the image as an unmodified PNG from the application's Iced renderer at 1×,
using the actual inspector content at its 246-pixel available width. The
fixture is a selected group containing a nitrogen ring, arrow, rectangle, and
fixed-size caption, plus an unselected oxygen. The renderer check clicks and
types `72` into Rotate, verifies Enter submits rotation, and clicks all six
Apply buttons at both 246- and 268-pixel content widths. The published image
shows `72` entered in Rotate; the test verifies emitted widget messages
separately from application updates.

Regenerate the fixture and renderer images with:

```sh
cargo test --bin reshiki numeric_transforms -- --include-ignored --nocapture
```

Outputs are in the system temporary directory under
`reshiki-numeric-transforms-qa/`. Open `numeric-transforms.rsk`, select the
group, expand Arrange & transform, then apply Rotate `72`, Scale `125`, and a
new width with the lock both on and off. Undo/Redo each step. Actual desktop
input validation is tracked separately in the draft PR and remains pending
until the combined integration build is checked.

## Regression coverage

The targeted tests cover six numeric operations, exact Undo/Redo and native
save/reopen, mixed groups and untouched objects, fixed publication styling,
physical dimensions with and without proportional locking, crossing fixed
labels, impossible and zero-extent sizes, non-finite and invalid input,
unchanged-value history, selection and Undo refresh, and native-engine
tetrahedral/alkene InChIKey preservation. The opt-in renderer test covers input,
Enter, Apply routing, and compact layout.

In the debug build, explicit width Apply took 17 ms for six selected atoms in
a 1000-atom drawing, 261 ms for all 1000 atoms, and 163 ms for the inward-label
fallback in a 1002-atom drawing. These local timings include bounds solving
and validation; they are not release-build performance guarantees. Ordinary
dimensions use bounded interpolation; the minimum search runs only when fixed
labels prevent the initial interval from reaching a smaller target.

Release caption: **Enter exact rotation, tilt, dimensions, and scale in the
selection inspector, with optional proportional resizing and one-step Undo.**
