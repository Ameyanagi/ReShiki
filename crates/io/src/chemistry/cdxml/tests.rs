use super::*;

fn substituted_double_xml(position: &str) -> String {
    format!(
        "<CDXML BondLength='30'><page><fragment id='1'>\
         <n id='2' p='0 0'/><n id='3' p='25.9808 -15'/>\
         <n id='4' p='51.9616 0'/><n id='5' p='77.9423 -15'/>\
         <b id='6' B='2' E='3'/><b id='7' B='3' E='4' Order='2' {position}/>\
         <b id='8' B='4' E='5'/></fragment></page></CDXML>"
    )
}

#[test]
fn omitted_cdxml_double_position_stays_auto_and_uses_a_chain_offset() {
    use crate::bonds::DoublePosition;

    let imported = import_cdxml(&substituted_double_xml("")).unwrap();
    let doc = &imported.document;
    let bond = doc.bonds.iter().find(|bond| bond.order == 2).unwrap();
    assert_eq!(bond.double_position, DoublePosition::Auto);
    assert!(matches!(
        crate::scene::effective_double_position(doc, bond),
        DoublePosition::Left | DoublePosition::Right
    ));
    let request = crate::engine::Request::molecule("export", doc.clone());
    let exported = crate::exchange::drawing::write(doc, (&request).into()).unwrap();
    let xml = roxmltree::Document::parse(&exported).unwrap();
    let double = xml
        .descendants()
        .find(|node| node.has_tag_name("b") && node.attribute("Order") == Some("2"))
        .unwrap();
    assert_eq!(double.attribute("DoublePosition"), None);
    let reopened = import_cdxml(&exported).unwrap().document;
    let bond = reopened.bonds.iter().find(|bond| bond.order == 2).unwrap();
    assert_eq!(bond.double_position, DoublePosition::Auto);
    assert!(matches!(
        crate::scene::effective_double_position(&reopened, bond),
        DoublePosition::Left | DoublePosition::Right
    ));
}

#[test]
fn omitted_cdxml_terminal_double_position_stays_centered() {
    use crate::bonds::DoublePosition;

    let xml = substituted_double_xml("").replace("<b id='8' B='4' E='5'/>", "");
    let imported = import_cdxml(&xml).unwrap();
    let doc = &imported.document;
    let bond = doc.bonds.iter().find(|bond| bond.order == 2).unwrap();
    assert_eq!(bond.double_position, DoublePosition::Auto);
    assert_eq!(
        crate::scene::effective_double_position(doc, bond),
        DoublePosition::Center
    );
}

#[test]
fn explicit_cdxml_double_positions_survive_import_and_export() {
    use crate::bonds::DoublePosition;

    for (name, expected) in [
        ("Left", DoublePosition::Left),
        ("Right", DoublePosition::Right),
        ("Center", DoublePosition::Center),
    ] {
        let imported =
            import_cdxml(&substituted_double_xml(&format!("DoublePosition='{name}'"))).unwrap();
        let doc = &imported.document;
        let bond = doc.bonds.iter().find(|bond| bond.order == 2).unwrap();
        assert_eq!(bond.double_position, expected, "{name}");
        assert_eq!(crate::scene::effective_double_position(doc, bond), expected);
        let request = crate::engine::Request::molecule("export", doc.clone());
        let exported = crate::exchange::drawing::write(doc, (&request).into()).unwrap();
        let xml = roxmltree::Document::parse(&exported).unwrap();
        let double = xml
            .descendants()
            .find(|node| node.has_tag_name("b") && node.attribute("Order") == Some("2"))
            .unwrap();
        assert_eq!(double.attribute("DoublePosition"), Some(name));
        let reopened = import_cdxml(&exported).unwrap().document;
        let bond = reopened.bonds.iter().find(|bond| bond.order == 2).unwrap();
        assert_eq!(bond.double_position, expected, "{name}");
    }
}

#[test]
fn predicate_defaults_and_preparation_error_precedence_are_preserved() {
    for &(name, defaults) in PREDICATES {
        let xml = |value| {
            format!(
                "<CDXML><page id=\"1\"><fragment id=\"2\"><n id=\"3\" p=\"0 0\" {name}=\"{value}\"/></fragment></page></CDXML>"
            )
        };
        assert!(matches!(
            read(&xml("query")),
            Err(Error::Unsupported("query or reaction predicate"))
        ));
        let error = prepare_cdxml(&xml("query")).unwrap_err();
        assert_eq!(error.stage, PreparationStage::Validation);
        assert!(matches!(error.cause, PreparationCause::Predicate(value) if value == name));
        for &default in defaults {
            read(&xml(default)).unwrap();
            assert!(!matches!(
                prepare_cdxml(&xml(default)),
                Err(PreparationError {
                    cause: PreparationCause::Predicate(_),
                    ..
                })
            ));
        }
    }
    // Predicate priority follows the fixed table, not XML attribute order, and
    // precedes unsupported objects even when the object comes first.
    let xml =
        "<CDXML><page><unsupported/><n RxnChange=\"yes\" RingBondCount=\"2\"/></page></CDXML>";
    let error = prepare_cdxml(xml).unwrap_err();
    assert_eq!(error.stage, PreparationStage::Validation);
    assert!(matches!(
        error.cause,
        PreparationCause::Predicate("RingBondCount")
    ));
}
