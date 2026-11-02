use super::*;
use crate::{
    chemistry::cdxml,
    graphics::{BracketSides, Graphic, GraphicKind},
    grouping::Group,
};

fn bracket_document(kind: GraphicKind, sides: BracketSides, text: Option<&str>) -> Document {
    let mut document = Document::default();
    let mut bracket = Graphic::dragged(
        1,
        kind,
        Point::new(-20., -30.),
        Point::new(90., 40.),
        Default::default(),
        sides,
        false,
    );
    bracket.superscript = text.map(str::to_string);
    document.graphics.push(bracket);
    document
}

fn export(document: &Document) -> String {
    let request = Request::molecule("export", document.clone());
    write(document, (&request).into()).unwrap()
}

#[test]
fn bracket_superscripts_export_as_grouped_uninterpreted_cdxml_and_cdx_text() {
    for kind in [
        GraphicKind::Brackets,
        GraphicKind::Parentheses,
        GraphicKind::Braces,
    ] {
        for sides in [BracketSides::Both, BracketSides::Right] {
            for text in ["‡", "+", "−", "•+", "•−", "2−", "custom <&>"] {
                let document = bracket_document(kind, sides, Some(text));
                let before = document.clone();
                let xml = export(&document);
                let decoded =
                    crate::exchange::from_cdx(&crate::exchange::to_cdx(&xml).unwrap()).unwrap();
                for xml in [&xml, &decoded] {
                    let tree = roxmltree::Document::parse(xml).unwrap();
                    let labels: Vec<_> = tree
                        .descendants()
                        .filter(|node| node.has_tag_name("t"))
                        .collect();
                    assert_eq!(labels.len(), 1);
                    let label = labels[0];
                    assert_eq!(label.attribute("InterpretChemically"), Some("no"));
                    let run = label
                        .children()
                        .find(|node| node.has_tag_name("s"))
                        .unwrap();
                    assert_eq!(run.text(), Some(text));
                    assert_eq!(
                        run.attribute("size").unwrap().parse::<f32>().unwrap(),
                        document.drawing_style.font_size_pt
                    );
                    assert_eq!(run.attribute("face"), Some("64"));
                    let group = label.parent().unwrap();
                    assert!(group.has_tag_name("group"));
                    assert_eq!(
                        group
                            .children()
                            .filter(|node| node.has_tag_name("curve"))
                            .count(),
                        if sides == BracketSides::Both { 2 } else { 1 }
                    );
                    let imported = cdxml::import_cdxml(xml).unwrap().document;
                    assert_eq!(imported.annotations.len(), 1);
                    assert_eq!(imported.annotations[0].text, text);
                    assert!(imported.groups.iter().any(|group| {
                        group.members.contains(&imported.annotations[0].id)
                            && group
                                .members
                                .iter()
                                .any(|id| imported.graphics.iter().any(|graphic| graphic.id == *id))
                    }));
                }
                assert_eq!(document, before);
            }
        }
    }
}

#[test]
fn absent_and_empty_bracket_superscripts_leave_existing_exchange_output_unchanged() {
    for sides in [BracketSides::Both, BracketSides::Left, BracketSides::Right] {
        let old = bracket_document(GraphicKind::Brackets, sides, None);
        let empty = bracket_document(GraphicKind::Brackets, sides, Some(""));
        assert_eq!(export(&old), export(&empty));
    }
}

#[test]
fn bracket_superscript_uses_script_face_at_the_minimum_allowed_nominal_font_size() {
    let mut document = bracket_document(GraphicKind::Brackets, BracketSides::Both, Some("‡"));
    document.drawing_style.font_size_pt = 4.;
    let imported = cdxml::import_cdxml(&export(&document)).unwrap().document;
    assert_eq!(imported.annotations[0].format.style.size_pt, 4.);
    assert_eq!(
        imported.annotations[0].format.style.script,
        crate::typography::Script::Superscript
    );
}

#[test]
fn bracketed_ion_charge_data_and_hidden_local_labels_survive_cdxml_and_cdx() {
    let mut document = Document::default();
    let carbon = document.add_atom("C", Point::default());
    let double = document.add_atom("O", Point::new(0., -42.));
    let left = document.add_atom("O", Point::new(-36., 21.));
    let right = document.add_atom("O", Point::new(36., 21.));
    document.add_bond(carbon, double, 2, "plain");
    document.add_bond(carbon, left, 1, "plain");
    document.add_bond(carbon, right, 1, "plain");
    for id in [left, right] {
        let atom = document.atom_mut(id).unwrap();
        atom.charge = -1;
        atom.display.hide_charge = true;
        atom.marks.push(crate::scientific::AtomMark {
            id: None,
            kind: crate::scientific::MarkKind::CircledCharge,
            offset: Point::new(15., -12.),
            angle: 0.,
            size_pt: None,
        });
    }
    let mut bracket = bracket_document(GraphicKind::Brackets, BracketSides::Both, Some("2−"))
        .graphics
        .remove(0);
    bracket.id = document.next_id();
    let bracket_id = bracket.id;
    document.graphics.push(bracket);
    document.groups.push(Group {
        id: document.next_id(),
        members: vec![carbon, double, left, right, bracket_id],
        integral: false,
    });
    let before = document.clone();
    let xml = export(&document);
    let decoded = crate::exchange::from_cdx(&crate::exchange::to_cdx(&xml).unwrap()).unwrap();
    for xml in [&xml, &decoded] {
        let tree = roxmltree::Document::parse(xml).unwrap();
        assert_eq!(
            tree.descendants()
                .filter(|node| node.has_tag_name("n") && node.attribute("Charge") == Some("-1"))
                .count(),
            2
        );
        assert_eq!(
            tree.descendants()
                .filter(
                    |node| node.has_tag_name("graphic") && node.attribute("Visible") == Some("no")
                )
                .count(),
            2
        );
        let imported = cdxml::import_cdxml(xml).unwrap().document;
        assert_eq!(
            imported.atoms.iter().map(|atom| atom.charge).sum::<i32>(),
            -2
        );
        assert_eq!(
            imported
                .atoms
                .iter()
                .filter(|atom| atom.charge == -1 && atom.display.hide_charge)
                .count(),
            2
        );
        assert!(imported.annotations.iter().any(|label| label.text == "2−"));
        // A reopened export keeps its hidden charges on the next export.
        let again = cdxml::import_cdxml(&export(&imported)).unwrap().document;
        assert_eq!(
            again
                .atoms
                .iter()
                .filter(|atom| atom.charge == -1 && atom.display.hide_charge)
                .count(),
            2
        );
    }
    assert_eq!(document, before);
}

#[test]
fn bracket_superscript_radical_ion_keeps_its_positioned_radical_when_local_charge_is_hidden() {
    let mut document = Document::default();
    let id = document.add_atom("N", Point::default());
    let atom = document.atom_mut(id).unwrap();
    atom.charge = 1;
    atom.radical_electrons = 1;
    atom.explicit_h = 3;
    atom.no_implicit = true;
    atom.label_h = 3;
    atom.display.hide_charge = true;
    atom.marks.push(crate::scientific::AtomMark {
        id: None,
        kind: crate::scientific::MarkKind::RadicalIon,
        offset: Point::new(-15., -12.),
        angle: 0.,
        size_pt: None,
    });
    let mut bracket = bracket_document(GraphicKind::Brackets, BracketSides::Both, Some("•+"))
        .graphics
        .remove(0);
    bracket.id = document.next_id();
    let bracket_id = bracket.id;
    document.graphics.push(bracket);
    document.groups.push(Group {
        id: document.next_id(),
        members: vec![id, bracket_id],
        integral: false,
    });
    let xml = export(&document);
    let decoded = crate::exchange::from_cdx(&crate::exchange::to_cdx(&xml).unwrap()).unwrap();
    for xml in [&xml, &decoded] {
        let tree = roxmltree::Document::parse(xml).unwrap();
        assert!(tree.descendants().any(|node| node.has_tag_name("graphic")
            && node.attribute("SymbolType") == Some("Electron")
            && node.attribute("Visible") != Some("no")));
        let imported = cdxml::import_cdxml(xml).unwrap().document;
        assert_eq!(imported.atoms[0].charge, 1);
        assert_eq!(imported.atoms[0].radical_electrons, 1);
        assert!(imported.atoms[0].display.hide_charge);
        assert_eq!(
            imported.atoms[0].marks[0].kind,
            crate::scientific::MarkKind::Radical
        );
    }
}
