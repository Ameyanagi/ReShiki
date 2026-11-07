# Rotated carbonyl label clearance

`acetophenone.mol` is a nine-atom acetophenone drawing generated from
`CC(=O)c1ccccc1` with ReShiki's native engine. Atom 2 is the carbonyl carbon
and atom 3 is oxygen. All bonds have the same drawing length.

For the matched renderer comparison, rigidly rotate all atom coordinates
about atom 2 so the C=O direction is 90°, 60° or 45° from positive X in the
MOL coordinate system. Preserve all atom distances, import the same rotated
MOL file in each build, and export PNG with the default JACS / ACS style.

The automated regressions in
`crates/model/src/scene/double_bond_tests.rs` exercise the same centered-double
rendering path with minimal C=O, N=O and O=O drawings. They use the bundled
Liberation Sans font for portable measurements of glyph clearance, rotation
geometry, small-margin label ownership and mechanism-arrow targeting.

See the [visual review](../../../docs/changes/carbonyl-rotation.md).
