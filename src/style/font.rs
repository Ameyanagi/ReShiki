/// Resolve the requested weight before reading advances, bounds or outlines.
/// A variable font's built-in default can be Thin even for a normal CSS request.
/// Static faces retain their existing metrics because they have no weight axis.
pub(super) fn face(bytes: &[u8], index: u32, weight: u16) -> Option<ttf_parser::Face<'_>> {
    let mut face = ttf_parser::Face::parse(bytes, index).ok()?;
    face.set_variation(ttf_parser::Tag::from_bytes(b"wght"), f32::from(weight));
    Some(face)
}

#[cfg(test)]
mod tests {
    const VARIABLE: &[u8] =
        include_bytes!("../../tests/fixtures/font-export-103/variable-default-100.subset.ttf");

    #[test]
    fn metrics_follow_requested_weight_instead_of_the_variable_default() {
        for (bytes, weight) in [
            (
                include_bytes!("../../tests/fixtures/font-export-103/static-400.subset.ttf")
                    .as_slice(),
                400,
            ),
            (
                include_bytes!("../../tests/fixtures/font-export-103/static-700.subset.ttf")
                    .as_slice(),
                700,
            ),
        ] {
            let variable = super::face(VARIABLE, 0, weight).unwrap();
            let reference = super::face(bytes, 0, weight).unwrap();
            assert_eq!(variable.ascender(), reference.ascender());
            for c in "HNO".chars() {
                let vg = variable.glyph_index(c).unwrap();
                let sg = reference.glyph_index(c).unwrap();
                assert_eq!(
                    variable.glyph_hor_advance(vg),
                    reference.glyph_hor_advance(sg)
                );
                let actual = variable.glyph_bounding_box(vg).unwrap();
                let expected = reference.glyph_bounding_box(sg).unwrap();
                for (a, e) in [
                    (actual.x_min, expected.x_min),
                    (actual.y_min, expected.y_min),
                    (actual.x_max, expected.x_max),
                    (actual.y_max, expected.y_max),
                ] {
                    // The variable outline has fractional points; static glyf
                    // coordinates are rounded to integer font units.
                    assert!((i32::from(a) - i32::from(e)).abs() <= 1);
                }
            }
        }
    }
}
