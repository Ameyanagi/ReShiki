use super::*;

const NATIVE: &[u8] = include_bytes!("../../tests/fixtures/tlc/chemdraw26-mixed-spots.cdx");

fn tlc_frame(object: u16, properties: &[(u16, &[u8])], unknown_child: bool) -> Vec<u8> {
    let mut bytes = HEADER.to_vec();
    for (code, id) in [(0x8000u16, 0u32), (0x8001, 1), (object, 2)] {
        bytes.extend_from_slice(&code.to_le_bytes());
        bytes.extend_from_slice(&id.to_le_bytes());
    }
    for (code, data) in properties {
        property(&mut bytes, *code, data).unwrap();
    }
    if unknown_child {
        bytes.extend_from_slice(&0x8fffu16.to_le_bytes());
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(&[0, 0]);
    }
    bytes.extend_from_slice(&[0; 8]);
    bytes
}

fn assert_native_plate(xml: &str) {
    let document = roxmltree::Document::parse(xml).unwrap();
    let plate = document
        .descendants()
        .find(|node| node.has_tag_name("tlcplate"))
        .unwrap();
    assert_eq!(plate.attribute("TopLeft"), Some("330 383"));
    assert_eq!(plate.attribute("BottomRight"), Some("370 583"));
    assert_eq!(plate.attribute("OriginFraction"), Some("0.1"));
    assert_eq!(plate.attribute("SolventFrontFraction"), Some("0.1"));
    assert_eq!(plate.attribute("ShowOrigin"), Some("yes"));
    assert_eq!(plate.attribute("ShowSolventFront"), Some("yes"));
    let lanes: Vec<_> = plate
        .children()
        .filter(|node| node.has_tag_name("tlclane"))
        .collect();
    assert_eq!(lanes.len(), 3);
    for (index, (lane, rf)) in lanes.iter().zip(["0.28", "0.51", "0.77"]).enumerate() {
        let spot = lane
            .children()
            .find(|node| node.has_tag_name("tlcspot"))
            .unwrap();
        assert_eq!(spot.attribute("Rf"), Some(rf));
        assert_eq!(spot.attribute("Width"), Some("325095"));
        assert_eq!(spot.attribute("Height"), Some("325095"));
        assert_eq!(spot.attribute("Tail"), Some("0"));
        assert_eq!(spot.attribute("ShowRf"), (index == 0).then_some("yes"));
        assert_eq!(spot.attribute("CurveType").is_some(), index != 2);
    }
    let tag = plate
        .descendants()
        .find(|node| node.has_tag_name("objecttag"))
        .unwrap();
    assert_eq!(tag.attribute("Name"), Some("rf"));
    let label: String = tag
        .descendants()
        .filter(|node| node.is_text())
        .filter_map(|node| node.text())
        .collect();
    assert_eq!(label, "Rf = 0.28");
    assert!(!xml.contains(TLC_SKIPPED));
}

#[test]
fn tlc_original_cdx_and_strict_roundtrip_keep_native_spot_dimensions() {
    let xml = from_cdx_tolerant(NATIVE).unwrap();
    assert_native_plate(&xml);
    let bytes = to_cdx(&xml).unwrap();
    assert_native_plate(&from_cdx(&bytes).unwrap());
    assert_native_plate(&from_cdx_tolerant(&bytes).unwrap());
}

#[test]
fn tlc_dimensions_keep_signed_integer_precision_without_changing_page_coordinates() {
    for (width, height) in [(325095, 131073), (i32::MIN, i32::MAX)] {
        let xml = format!(
            "<CDXML><page id='1' Width='540.5' Height='720.25'>\
             <tlcplate id='2'><tlclane id='3'><tlcspot id='4' Width='{width}' Height='{height}'/>\
             </tlclane></tlcplate></page></CDXML>"
        );
        let restored = from_cdx(&to_cdx(&xml).unwrap()).unwrap();
        let document = roxmltree::Document::parse(&restored).unwrap();
        let page = document
            .descendants()
            .find(|node| node.has_tag_name("page"))
            .unwrap();
        assert_eq!(page.attribute("Width"), Some("540.5"));
        assert_eq!(page.attribute("Height"), Some("720.25"));
        let spot = document
            .descendants()
            .find(|node| node.has_tag_name("tlcspot"))
            .unwrap();
        assert_eq!(spot.attribute("Width"), Some(width.to_string().as_str()));
        assert_eq!(spot.attribute("Height"), Some(height.to_string().as_str()));
    }
    for dimension in ["1.5", "2147483648", "-2147483649", "NaN"] {
        let xml = format!("<CDXML><tlcspot Width='{dimension}'/></CDXML>");
        assert!(to_cdx(&xml).is_err(), "accepted {dimension}");
    }
}

#[test]
fn tlc_tolerant_unknown_and_malformed_data_mark_only_the_affected_element() {
    let nan = f64::NAN.to_le_bytes();
    for object in [0x801d, 0x801e, 0x801f] {
        for (code, data) in [
            (0x7fff, &[1u8][..]),
            (0x0812, &[1, 2, 3][..]),
            (0x0ab0, &nan[..]),
            (0x0ab2, &[2][..]),
        ] {
            let bytes = tlc_frame(object, &[(code, data)], false);
            assert!(from_cdx(&bytes).is_err());
            let xml = from_cdx_tolerant(&bytes).unwrap();
            let document = roxmltree::Document::parse(&xml).unwrap();
            let marked: Vec<_> = document
                .descendants()
                .filter(|node| node.attribute(TLC_SKIPPED) == Some("yes"))
                .collect();
            assert_eq!(marked.len(), 1);
            let name = OBJECTS.iter().find(|(code, _)| *code == object).unwrap().1;
            assert!(marked.first().unwrap().has_tag_name(name));
            assert!(
                to_cdx(&xml).is_err(),
                "a damage marker cannot be lost on export"
            );
        }
        let bytes = tlc_frame(object, &[], true);
        assert!(from_cdx(&bytes).is_err());
        assert!(from_cdx_tolerant(&bytes).unwrap().contains(TLC_SKIPPED));
    }
    let ordinary = tlc_frame(0x8007, &[(0x7fff, &[1])], false);
    assert!(!from_cdx_tolerant(&ordinary).unwrap().contains(TLC_SKIPPED));
}

#[test]
fn tlc_successful_color_fallback_and_native_dimensions_are_not_marked() {
    let width = 325095i32.to_le_bytes();
    let bytes = tlc_frame(0x801f, &[(0x0812, &width), (0x0301, &[3, 0, 0, 0])], false);
    let xml = from_cdx_tolerant(&bytes).unwrap();
    assert!(xml.contains("Width=\"325095\""));
    assert!(xml.contains("color=\"3\""));
    assert!(!xml.contains(TLC_SKIPPED));
}

#[test]
fn tlc_nested_skipped_data_marks_its_tlc_ancestor_without_marking_ordinary_tags() {
    let mut bytes = tlc_frame(0x801f, &[], false);
    bytes.truncate(bytes.len() - 8);
    bytes.extend_from_slice(&0x8011u16.to_le_bytes());
    bytes.extend_from_slice(&3u32.to_le_bytes());
    property(&mut bytes, 0x7fff, &[1]).unwrap();
    bytes.extend_from_slice(&[0; 10]);
    let xml = from_cdx_tolerant(&bytes).unwrap();
    let document = roxmltree::Document::parse(&xml).unwrap();
    let spot = document
        .descendants()
        .find(|node| node.has_tag_name("tlcspot"))
        .unwrap();
    assert_eq!(spot.attribute(TLC_SKIPPED), Some("yes"));
    let tag = document
        .descendants()
        .find(|node| node.has_tag_name("objecttag"))
        .unwrap();
    assert_eq!(tag.attribute(TLC_SKIPPED), None);
}
