use moruno::{
    atom_labels::{self, Carbons, HydrogenPosition, Number, Owner},
    document::{Document, Point},
    editing,
    engine::{PythonEngine, Request},
    scene,
};

fn number(text: &str) -> Number {
    Number {
        text: text.into(),
        offset: None,
        style: atom_labels::number_style(),
    }
}

#[test]
fn sequences_support_numeric_prefixes_latin_and_greek_rollover() {
    assert_eq!(
        atom_labels::sequence("atom9", 3).unwrap(),
        ["atom9", "atom10", "atom11"]
    );
    assert_eq!(atom_labels::sequence("z", 3).unwrap(), ["z", "aa", "ab"]);
    assert_eq!(atom_labels::sequence("ω", 3).unwrap(), ["ω", "αα", "αβ"]);
    assert_eq!(atom_labels::sequence("Cα9", 2).unwrap(), ["Cα9", "Cα10"]);
    assert!(atom_labels::sequence("two words", 2).is_err());
}

#[tokio::test]
async fn appearance_leaves_formula_and_identity_intact() {
    let engine = PythonEngine::default();
    let response = engine
        .request(Request::import_smiles("CCCO"))
        .await
        .unwrap();
    let original = response.document.unwrap();
    let mut doc = original.clone();
    assert!(!atom_labels::visible(&doc.atoms[0], &doc));
    doc.atom_labels.carbons = Carbons::Terminal;
    assert!(atom_labels::visible(&doc.atoms[0], &doc));
    assert!(!atom_labels::visible(&doc.atoms[1], &doc));
    doc.atom_labels.hydrogens = false;
    doc.atoms[3].display.hydrogen_position = HydrogenPosition::Above;
    doc.atoms[3].display.number = Some(number("O1"));
    let result = engine
        .request(Request::molecule("analyze", doc.clone()))
        .await
        .unwrap();
    assert_eq!(
        result.analysis.unwrap().smiles,
        response.analysis.unwrap().smiles
    );
    assert_eq!(doc.atoms.len(), original.atoms.len());
    let rendered: Vec<_> = scene::primitives(&doc)
        .into_iter()
        .filter_map(|p| match p {
            scene::Primitive::Text { text, .. } => Some(text),
            _ => None,
        })
        .collect();
    assert!(!rendered.iter().any(|t| t == "H"));
    assert!(rendered.iter().any(|t| t == "O1"));
    let decoded: Document = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
    assert_eq!(decoded, doc);
}

#[test]
fn indicators_move_transform_and_copy_with_their_owners() {
    let mut doc = Document::default();
    let a = doc.add_atom("N", Point::new(10., 20.));
    doc.atom_mut(a).unwrap().display.number = Some(number("N7"));
    Owner::Number(a).set_offset(&mut doc, Some(Point::new(0., -40.)));
    let indicators = atom_labels::indicators(&doc);
    assert_eq!(indicators[0].center, Point::new(10., -20.));
    let bounds = scene::selection_bounds(&doc, &[a]).unwrap();
    assert!(bounds.0.y <= indicators[0].origin.y);
    editing::transform_about(&mut doc, &[a], Point::new(10., 20.), 2., 90.);
    let transformed = atom_labels::indicators(&doc)[0].center;
    assert!((transformed.x - 90.).abs() < 0.001);
    assert!((transformed.y - 20.).abs() < 0.001);
    let mut target = Document::default();
    target.atom_labels.hydrogens = false;
    let ids = editing::append(&mut target, &doc, Point::new(100., 100.));
    let copied = atom_labels::indicators(&target);
    assert_eq!(copied[0].owner, Owner::Number(ids[0]));
    assert!((copied[0].center.x - 190.).abs() < 0.001);
    assert_eq!(target.atoms[0].display.hydrogens, Some(true));
}

#[tokio::test]
async fn saved_chemdraw_fixture_preserves_owned_indicators_and_chemistry() {
    let engine = PythonEngine::default();
    let result = engine
        .request(Request::import(
            "cdxml",
            include_str!("fixtures/atom-labels-chemdraw.cdxml"),
        ))
        .await
        .unwrap();
    let doc = result.document.unwrap();
    assert_eq!(result.analysis.as_ref().unwrap().formula, "C9H11NO2");
    assert!(doc.annotations.is_empty());
    assert_eq!(atom_labels::indicators(&doc).len(), 13);
    assert_eq!(
        doc.atoms
            .iter()
            .filter(|a| a.cip_label.as_deref() == Some("S"))
            .count(),
        1
    );
    assert!(doc.atoms.iter().all(|a| a.display.hydrogens == Some(false)));
    let mut export = Request::molecule("export", doc.clone());
    export.format = Some("cdxml".into());
    let xml = engine.request(export).await.unwrap().output.unwrap();
    assert!(xml.contains("objecttag"));
    let imported = engine
        .request(Request::import("cdxml", &xml))
        .await
        .unwrap();
    assert_eq!(
        imported.analysis.unwrap().inchikey,
        result.analysis.unwrap().inchikey
    );
    let back = imported.document.unwrap();
    assert!(back.annotations.is_empty());
    for (a, b) in atom_labels::indicators(&doc)
        .iter()
        .zip(atom_labels::indicators(&back))
    {
        assert_eq!(a.text, b.text);
        let original_offset = a.center.offset(
            -a.owner.anchor(&doc).unwrap().x,
            -a.owner.anchor(&doc).unwrap().y,
        );
        let new_offset = b.center.offset(
            -b.owner.anchor(&back).unwrap().x,
            -b.owner.anchor(&back).unwrap().y,
        );
        assert!(original_offset.distance(new_offset) < 0.01);
    }
}

#[tokio::test]
async fn cip_is_computed_and_exports_refresh_stale_cached_labels() {
    let engine = PythonEngine::default();
    for (smiles, expected) in [
        ("N[C@@H](Cc1ccccc1)C(O)=O", "S"),
        ("N[C@H](Cc1ccccc1)C(O)=O", "R"),
        ("F/C=C/F", "E"),
        ("F/C=C\\F", "Z"),
    ] {
        let mut doc = engine
            .request(Request::import_smiles(smiles))
            .await
            .unwrap()
            .document
            .unwrap();
        doc.atom_labels.stereo = true;
        assert!(
            atom_labels::indicators(&doc)
                .iter()
                .any(|i| i.text == format!("({expected})"))
        );
        atom_labels::clear_computed(&mut doc);
        assert!(atom_labels::indicators(&doc).is_empty());
        let refreshed = moruno::export::checked_document(&engine, doc)
            .await
            .unwrap();
        assert!(scene::svg(&refreshed).contains(&format!("({expected})")));
    }
}

#[tokio::test]
async fn automatic_numbers_reserve_later_manual_stereo_labels() {
    let engine = PythonEngine::default();
    let mut doc = engine
        .request(Request::import(
            "cdxml",
            include_str!("fixtures/atom-labels-chemdraw.cdxml"),
        ))
        .await
        .unwrap()
        .document
        .unwrap();
    for atom in &mut doc.atoms {
        atom.display.hydrogens = Some(true);
        atom.display.number.as_mut().unwrap().offset = None;
    }
    let items = atom_labels::indicators(&doc);
    let stereo = items
        .iter()
        .find(|i| matches!(i.owner, Owner::AtomStereo(_)))
        .unwrap();
    for number in items.iter().filter(|i| matches!(i.owner, Owner::Number(_))) {
        let overlap_x = (number.origin.x + number.width).min(stereo.origin.x + stereo.width)
            - number.origin.x.max(stereo.origin.x);
        let overlap_y = (number.origin.y + number.height).min(stereo.origin.y + stereo.height)
            - number.origin.y.max(stereo.origin.y);
        assert!(
            overlap_x <= 0. || overlap_y <= 0.,
            "{} overlaps manual stereo",
            number.text
        );
    }
}
