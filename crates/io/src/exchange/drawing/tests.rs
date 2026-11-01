use super::*;

fn text_document(label: &str) -> Document {
    let mut document = Document::default();
    let left = document.add_atom("*", Point::default());
    document.atom_mut(left).unwrap().display.variable = Some(label.into());
    let right = document.add_atom("C", Point::new(42., 0.));
    document.add_bond(left, right, 1, "plain");
    document
}

#[test]
fn strict_text_label_refusal_names_the_label_without_calling_the_drawing_invalid() {
    let document = text_document("MeO");
    let before = document.clone();
    for preserving in [false, true] {
        let error = write_impl(&document, Options::default(), preserving, false).unwrap_err();
        assert!(matches!(error, Error::Unsupported(_)));
        let message = error.to_string();
        assert!(message.contains("text label “MeO”"), "{message}");
        assert!(message.ends_with(TEXT_LABEL_COPY_ADVICE));
        assert!(!message.contains("Invalid") && !message.contains("ring"));
    }
    let (xml, _) = write_clipboard(&document).unwrap();
    assert!(xml.contains("MeO"), "Ordinary Copy still retains atom text");
    assert_eq!(document, before);
}

#[test]
fn strict_ring_curve_refusal_does_not_claim_a_text_label_is_present() {
    let mut document = crate::rings::Preset::Benzene.document(42., true);
    let selected = document
        .atoms
        .iter()
        .take(3)
        .map(|a| a.id)
        .collect::<Vec<_>>();
    crate::ring_arcs::toggle(&mut document, &selected).unwrap();
    let before = document.clone();
    let error = write(&document, Options::default()).unwrap_err();
    assert!(matches!(error, Error::Unsupported(_)));
    let message = error.to_string();
    assert!(message.contains("inner ring curves"), "{message}");
    assert!(!message.contains("text label") && !message.contains("Invalid"));
    assert_eq!(document, before);
}

#[test]
fn refusal_excerpts_are_bounded_and_cannot_inject_lines_quotes_or_direction_changes() {
    assert_eq!(label_excerpt("\t MeO\n  label\r"), "MeO label");
    assert_eq!(label_excerpt("“MeO”\u{202e}\"\u{0007}"), "'MeO''");
    assert_eq!(label_excerpt("\u{2067}\n\t"), "(empty)");
    let long = "酸".repeat(200);
    assert_eq!(label_excerpt(&long), format!("{}…", "酸".repeat(24)));
    let mut document = text_document(&"酸".repeat(32));
    for label in ["second", "third"] {
        let id = document.add_atom("*", Point::new(100., 0.));
        document.atom_mut(id).unwrap().display.variable = Some(label.into());
    }
    let message = write(&document, Options::default())
        .unwrap_err()
        .to_string();
    assert!(message.contains("and 2 more"), "{message}");
    assert!(message.chars().count() < 190, "{message}");
}

#[test]
fn malformed_documents_still_report_invalid() {
    let mut document = text_document("MeO");
    document.atoms[1].id = document.atoms[0].id;
    assert!(matches!(
        write(&document, Options::default()),
        Err(Error::Invalid(_))
    ));
}

#[test]
fn centered_aromatic_charge_export_retains_atom_charge_without_extra_center_text() {
    let mut doc = Document::default();
    let ids = crate::editing::ring(&mut doc, Point::default(), 7, false, 5.);
    doc.atom_mut(ids[0]).unwrap().charge = 1;
    for (i, bond) in doc.bonds.iter_mut().enumerate() {
        bond.order = if i % 2 == 1 { 2 } else { 1 };
    }
    let mut doc = crate::chemistry::document::aromatic_display(&doc, &ids)
        .unwrap()
        .finish()
        .unwrap()
        .document;
    assert!(crate::aromatic::toggle_center_charge(&mut doc, &ids).unwrap());
    let before = doc.clone();
    let xml = write(&doc, Options::default()).unwrap();
    let tree = roxmltree::Document::parse(&xml).unwrap();
    let charges: Vec<_> = tree
        .descendants()
        .filter(|node| node.has_tag_name("n"))
        .filter_map(|node| node.attribute("Charge"))
        .collect();
    assert_eq!(charges, ["1"]);
    assert!(!tree.descendants().any(|node| node.has_tag_name("t")));
    assert_eq!(doc, before);
}
