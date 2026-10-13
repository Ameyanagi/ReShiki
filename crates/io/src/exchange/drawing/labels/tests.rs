use super::*;
use crate::chemistry::cdxml;

fn dummy(label: &str) -> Document {
    let mut document = Document::default();
    let id = document.add_atom("*", Point::default());
    document.atom_mut(id).unwrap().display.variable = Some(label.into());
    document
}

fn runs(xml: &str, kind: &str) -> Vec<(String, u8)> {
    let tree = roxmltree::Document::parse(xml).unwrap();
    let atom = tree
        .descendants()
        .find(|node| node.has_tag_name("n") && node.attribute("NodeType") == Some(kind))
        .unwrap();
    let text = atom.children().find(|node| node.has_tag_name("t")).unwrap();
    text.children()
        .filter(|node| node.has_tag_name("s"))
        .map(|node| {
            (
                node.text().unwrap_or_default().into(),
                node.attribute("face").unwrap().parse().unwrap(),
            )
        })
        .collect()
}

fn import_variable(xml: &str) -> Document {
    let (xml, variables) = cdxml::drawing_variables(xml).unwrap();
    let prepared = cdxml::prepare_drawing(&xml).unwrap();
    let mut document = cdxml::assemble_cdxml(&prepared)
        .unwrap()
        .into_unchecked_drawing()
        .unwrap();
    variables.restore(&prepared, &mut document).unwrap();
    document
}

fn assert_variable_roundtrip(xml: &str, label: &str) {
    let document = import_variable(xml);
    assert_eq!(document.atoms.len(), 1);
    let atom = document.atoms.first().unwrap();
    assert_eq!(atom.display.variable.as_deref(), Some(label));
    assert_eq!(atom.element, "*");
    assert_eq!(atom.charge, 0);
}

#[test]
fn cdxml_dummy_labels_use_explicit_scripts_and_preserve_source_text() {
    for (label, expected) in [
        ("M2+", vec![("M", 0), ("2+", 64)]),
        ("SO4 2-", vec![("SO", 0), ("4", 32), (" 2-", 64)]),
    ] {
        let document = dummy(label);
        let (xml, _) = super::super::write_clipboard(&document).unwrap();
        assert_eq!(
            runs(&xml, "Unspecified"),
            expected
                .into_iter()
                .map(|(text, face)| (text.to_owned(), face))
                .collect::<Vec<_>>(),
            "{label}"
        );
        assert_variable_roundtrip(&xml, label);
    }
}

#[test]
fn cdx_dummy_labels_reuse_cdxml_script_runs_and_preserve_source_text() {
    for label in ["M2+", "SO4 2-"] {
        let (xml, _) = super::super::write_clipboard(&dummy(label)).unwrap();
        let bytes = crate::exchange::to_cdx(&xml).unwrap();
        let decoded = crate::exchange::from_cdx(&bytes).unwrap();
        assert_eq!(
            runs(&decoded, "Unspecified"),
            runs(&xml, "Unspecified"),
            "{label}"
        );
        assert_variable_roundtrip(&decoded, label);
    }
}

#[test]
fn dummy_labels_without_scripts_keep_the_existing_single_run() {
    for (label, style, face) in [
        ("Me", TextStyle::default(), 0),
        ("OH", TextStyle::default(), 96),
        (
            "X",
            TextStyle {
                formula: true,
                ..Default::default()
            },
            96,
        ),
        (
            "X",
            TextStyle {
                script: Script::Superscript,
                ..Default::default()
            },
            64,
        ),
    ] {
        let mut document = dummy(label);
        document.atoms.first_mut().unwrap().text_style = Some(style);
        let (xml, _) = super::super::write_clipboard(&document).unwrap();
        assert_eq!(runs(&xml, "Unspecified"), vec![(label.into(), face)]);
    }
}

#[test]
fn explicit_dummy_scripts_preserve_base_font_and_face_styles() {
    let mut document = dummy("M2+");
    document.atoms.first_mut().unwrap().text_style = Some(TextStyle {
        family: "Times New Roman".into(),
        size_pt: 12.,
        bold: true,
        italic: true,
        underline: true,
        formula: true,
        ..Default::default()
    });
    let (xml, _) = super::super::write_clipboard(&document).unwrap();
    assert_eq!(
        runs(&xml, "Unspecified"),
        vec![("M".into(), 7), ("2+".into(), 71)]
    );
    let tree = roxmltree::Document::parse(&xml).unwrap();
    let font = tree
        .descendants()
        .find(|node| node.has_tag_name("font") && node.attribute("name") == Some("Times New Roman"))
        .unwrap()
        .attribute("id")
        .unwrap();
    for run in tree.descendants().filter(|node| node.has_tag_name("s")) {
        assert_eq!(run.attribute("font"), Some(font));
        assert_eq!(run.attribute("size"), Some("12.0"));
    }
}

#[test]
fn generic_owned_labels_keep_their_existing_runs() {
    let mut document = Document::default();
    let id = document.add_atom("C", Point::default());
    document.atom_mut(id).unwrap().display.variable = Some("M2+".into());
    let left = document.add_atom("O", Point::new(-42., 0.));
    let right = document.add_atom("O", Point::new(42., 0.));
    document.add_bond(left, id, 1, "plain");
    document.add_bond(id, right, 1, "plain");
    document.generic.links.push(crate::generic::LinkNode {
        atom: id,
        min: 1,
        max: 2,
    });
    let (xml, _) = super::super::write_clipboard(&document).unwrap();
    assert_eq!(runs(&xml, "LinkNode"), vec![("M2+".into(), 0)]);
    let decoded = crate::exchange::from_cdx(&crate::exchange::to_cdx(&xml).unwrap()).unwrap();
    assert_eq!(runs(&decoded, "LinkNode"), vec![("M2+".into(), 0)]);
}
