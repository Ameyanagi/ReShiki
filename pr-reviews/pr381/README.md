# Issue #379 evidence: aimed attached arrowhead vs bond ink

Real renders from ReShiki's own figure exporter (`reshiki_io::export::drawing(doc, "png")`,
the same SVG -> resvg path as File > Export PNG), built from two source trees:

- `before/`: `origin/main` at `036f0c82` (extracted with `git archive`)
- `after/`: branch `fix/aimed-arrowhead-clearance` (commit `645596c4`; the later commits change only the fallback when no gap clears and the changelog, and leave this render unchanged)

## Scene (identical on both)

Harness: `harness/aim379.rs`, dropped temporarily into `crates/io/examples/` of each tree
and run with `cargo run -p reshiki-io --example aim379 -- <out dir>`.

1. Acetone drawn with C=O up (C at the origin, O at 270 deg, methyls at 150/30 deg, 42-unit bonds),
   methoxide (CH3-O-, lone pair facing acetone) to the left. Default document style.
2. `arrow_anchors::create` from the lone pair to the carbonyl carbon, Curved preset
   ("Attach ends to atoms and bonds"). Rendered as `default.png`; byte-identical on both trees
   (the #312 default arrival is untouched).
3. Aim the end: the end tangent square (handle 4, `Arrival`) is moved to 20 units from the end at
   235 deg (up-left, toward the C=O axis). The attachment is left alone.
   - before: `Arrow::edit_handle(4, p)`, exactly what main's canvas commit does;
   - after: `arrow_anchors::edit_handle(doc, id, 4, p)`, what the branch's canvas commit and drag
     preview both call.
   Both then run `transaction::reconcile` like a history commit, and are rendered as `aimed.png`.

## Files

| File | What it shows |
| --- | --- |
| `before/default.png`, `after/default.png` | Default attached arrow (identical) |
| `before/aimed.png` | main: the aimed head lies across the left C=O stroke and the C junction |
| `after/aimed.png` | branch: same aim; the end resolves along the aimed direction and the head clears all bond ink |
| `before/aimed-head-crop.png`, `after/aimed-head-crop.png` | 260x220 px crop at (800, 250) of `aimed.png`, upscaled 3x with `sips -z 660 780` |

Endpoint data printed by the harness:

- before: end (-2.63, 4.55), arrival control (-14.10, -11.84), anchor direction (-0.50, 0.87), gap 3.5
- after: end (-6.91, -1.57), arrival control (-18.38, -17.95), anchor direction (-0.57, -0.82), gap 3.5

The aimed tangent `control - end` is the same vector (-11.47, -16.38) in both: the aim is kept, only the
end resolved to the aimed side of the carbon. The target stays `Atom { atom: 1 }`.
