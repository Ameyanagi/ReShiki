use super::*;

#[test]
fn mechanism_curvature_91_before_rejects_independent_cubic_controls() {
    let xml = include_str!(
        "../../../../../../tests/fixtures/mechanism-curvature-91/independent-controls.cdxml"
    );
    let reader = ArrowReader::new(xml).unwrap();
    let colors = [NativeColor([0.; 3]); 4];
    let error = reader.read(4, 1., &colors, 1).unwrap_err();
    assert_eq!(
        error.to_string(),
        "This cubic arrow cannot be represented by a single quadratic bend"
    );
}
