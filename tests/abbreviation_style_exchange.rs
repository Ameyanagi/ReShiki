//! Native contracted text and the atom inside it are independent presentations.
use reshiki::{
    canvas_theme,
    chemistry::cdxml::import_cdxml,
    document::Document,
    exchange::{drawing, from_cdx, to_cdx},
    palette::Color,
    typography::TextStyle,
};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
const NATIVE: &str =
    include_str!("fixtures/structure-highlights/native-independent-label-ink.cdxml");

fn native() -> Result<Document> {
    Ok(import_cdxml(NATIVE)?.document)
}

fn editable_roundtrips(doc: &Document) -> Result<Vec<Document>> {
    let xml = drawing::write(doc, Default::default())?;
    Ok(vec![
        Document::from_json(&doc.file_json()?)?,
        import_cdxml(&xml)?.document,
        import_cdxml(&from_cdx(&to_cdx(&xml)?)?)?.document,
    ])
}

fn color(node: roxmltree::Node<'_, '_>, name: &str) -> Result<[u8; 3]> {
    let index = node
        .attribute(name)
        .ok_or("color attribute")?
        .parse::<usize>()?;
    if index < 2 {
        return Ok(if index == 0 { [0; 3] } else { [255; 3] });
    }
    let entry = node
        .document()
        .descendants()
        .find(|n| n.has_tag_name("colortable"))
        .and_then(|n| {
            n.children()
                .filter(|n| n.has_tag_name("color"))
                .nth(index - 2)
        })
        .ok_or("color table entry")?;
    let mut rgb = [0; 3];
    for (value, key) in rgb.iter_mut().zip(["r", "g", "b"]) {
        *value = (entry
            .attribute(key)
            .ok_or("color channel")?
            .parse::<f64>()?
            * 255.)
            .round() as u8;
    }
    Ok(rgb)
}

fn assert_written_style(node: roxmltree::Node<'_, '_>, expected: &TextStyle) -> Result {
    let run = node
        .children()
        .find(|n| n.has_tag_name("t"))
        .and_then(|n| n.children().find(|n| n.has_tag_name("s")))
        .ok_or("label run")?;
    assert_eq!(color(run, "color")?, expected.color.rgb());
    let font = node
        .document()
        .descendants()
        .find(|n| n.has_tag_name("font") && n.attribute("id") == run.attribute("font"))
        .and_then(|n| n.attribute("name"))
        .ok_or("font name")?;
    assert_eq!(font, expected.family);
    assert_eq!(
        run.attribute("size").ok_or("font size")?.parse::<f32>()?,
        expected.size_pt
    );
    let face = run.attribute("face").ok_or("font face")?.parse::<u8>()?;
    assert_eq!(face & 1 != 0, expected.bold);
    assert_eq!(face & 2 != 0, expected.italic);
    assert_eq!(face & 4 != 0, expected.underline);
    Ok(())
}

fn assert_editable_styles(
    doc: &Document,
    label: &TextStyle,
    atom: &TextStyle,
    label_halo: [u8; 3],
) -> Result {
    let written = drawing::write(doc, Default::default())?;
    for written in [written.clone(), from_cdx(&to_cdx(&written)?)?] {
        let xml = roxmltree::Document::parse(&written)?;
        let wrapper = xml
            .descendants()
            .find(|n| n.attribute("NodeType") == Some("Fragment"))
            .ok_or("contracted wrapper")?;
        let inner = wrapper
            .descendants()
            .find(|n| n.has_tag_name("n") && n.attribute("Element") == Some("8"))
            .ok_or("internal oxygen")?;
        assert_written_style(wrapper, label)?;
        assert_written_style(inner, atom)?;
        assert_eq!(color(wrapper, "highlightColor")?, label_halo);
        assert_eq!(color(inner, "highlightColor")?, [255; 3]);
    }
    Ok(())
}

fn assert_structure(doc: &Document) {
    assert_eq!(doc.atoms.len(), 3);
    assert_eq!(doc.bonds.len(), 2);
    assert_eq!(
        doc.atoms
            .iter()
            .map(|a| a.element.as_str())
            .collect::<Vec<_>>(),
        ["C", "O", "C"]
    );
    assert!(doc.atoms.iter().all(|a| a.charge == 0));
    assert!(doc.bonds.iter().all(|b| b.order == 1));
    assert_eq!(
        doc.atoms
            .iter()
            .map(|a| a.display.highlight)
            .collect::<Vec<_>>(),
        [
            None,
            Some(Color::Custom([255; 3])),
            Some(Color::Custom([129, 229, 255]))
        ]
    );
    assert_eq!(
        doc.bonds.iter().filter(|b| b.highlight.is_none()).count(),
        1
    );
    assert_eq!(
        doc.bonds
            .iter()
            .filter(|b| b.highlight == Some(Color::Custom([255, 197, 0])))
            .count(),
        1
    );
    assert_eq!(doc.drawing_style.font_family, "Helvetica");
    assert_eq!(doc.drawing_style.font_size_pt, 10.);
    assert_eq!(doc.drawing_style.bond_length_pt, 28.);
}

#[test]
fn native_boc_and_ome_keep_independent_fonts() -> Result {
    let text = from_cdx(include_bytes!("fixtures/abbreviations-native.cdx"))?;
    let xml = roxmltree::Document::parse(&text)?;
    let root = xml.root_element();
    let label = TextStyle {
        family: "Arial".into(),
        size_pt: 10.,
        color: Color::Ink,
        ..Default::default()
    };
    let inner = TextStyle {
        family: "Helvetica".into(),
        ..label.clone()
    };
    // The binary fixture's wrappers carry font 60 (Arial) and color 3 (black).
    // Its inner carbon inherits Helvetica/black; its oxygen has an explicit
    // Helvetica/color-0 run. These are source declarations, not app defaults.
    let document_font = xml
        .descendants()
        .find(|node| {
            node.has_tag_name("font") && node.attribute("id") == root.attribute("LabelFont")
        })
        .and_then(|node| node.attribute("name"));
    assert_eq!(document_font, Some("Helvetica"));
    assert_eq!(root.attribute("LabelColor"), Some("3"));
    let wrappers: Vec<_> = xml
        .descendants()
        .filter(|node| node.attribute("NodeType") == Some("Fragment"))
        .collect();
    assert_eq!(wrappers.len(), 2);
    for (wrapper, expected) in wrappers.into_iter().zip(["Boc", "OMe"]) {
        let run = wrapper
            .children()
            .find(|node| node.has_tag_name("t"))
            .and_then(|node| node.children().find(|node| node.has_tag_name("s")))
            .ok_or("native label run")?;
        assert_eq!(run.text(), Some(expected));
        assert_written_style(wrapper, &label)?;
    }

    let source = import_cdxml(&text)?.document;
    for doc in std::iter::once(source.clone()).chain(editable_roundtrips(&source)?) {
        assert_eq!(doc.abbreviations.len(), 2);
        for expected in ["Boc", "OMe"] {
            let group = doc
                .abbreviations
                .iter()
                .find(|group| group.label == expected)
                .ok_or("native abbreviation")?;
            assert_eq!(group.label_style.as_ref(), Some(&label), "{expected}");
            assert!(group.label_color_override, "{expected}");
            let anchor = doc.atom(group.anchor).ok_or("native anchor")?;
            assert_eq!(anchor.text_style.as_ref(), Some(&inner), "{expected}");
            assert!(anchor.display.color_override, "{expected}");
        }
    }
    Ok(())
}

#[test]
fn native_prime_independent_label_ink_survives_import_save_exchange_and_expansion() -> Result {
    let source = native()?;
    let before = source.clone();
    let label = TextStyle {
        family: "Helvetica".into(),
        color: Color::Custom([124; 3]),
        ..Default::default()
    };
    let atom = TextStyle {
        family: "Helvetica".into(),
        color: Color::Ink,
        ..Default::default()
    };
    for mut doc in std::iter::once(source.clone()).chain(editable_roundtrips(&source)?) {
        assert_structure(&doc);
        let group = doc.abbreviations.first().ok_or("OMe group")?;
        assert_eq!(doc.abbreviations.len(), 1);
        assert_eq!(group.label, "OMe");
        assert_eq!(group.label_style.as_ref(), Some(&label));
        assert!(group.label_color_override);
        let anchor = group.anchor;
        assert_eq!(
            doc.atom(anchor).ok_or("O")?.text_style.as_ref(),
            Some(&atom)
        );
        assert_eq!(
            canvas_theme::atom_color(&doc, doc.atom(anchor).ok_or("O")?),
            [124; 3]
        );
        assert_editable_styles(&doc, &label, &atom, [0; 3])?;
        let atoms = doc.atoms.clone();
        let bonds = doc.bonds.clone();
        assert_eq!(doc.expand_abbreviations(&[anchor]), 1);
        assert_eq!(
            doc.atoms, atoms,
            "Expansion keeps the internal atom presentation"
        );
        assert_eq!(doc.bonds, bonds);
        assert_eq!(
            canvas_theme::atom_color(&doc, doc.atom(anchor).ok_or("O")?),
            [0; 3]
        );
        assert_structure(&doc);
        let xml = drawing::write(&doc, Default::default())?;
        let xml = roxmltree::Document::parse(&xml)?;
        assert!(
            !xml.descendants()
                .any(|n| n.attribute("NodeType") == Some("Fragment"))
        );
        assert_written_style(
            xml.descendants()
                .find(|n| n.attribute("Element") == Some("8"))
                .ok_or("expanded O")?,
            &atom,
        )?;
    }
    assert_eq!(source, before, "All exporters preserve the editable source");
    Ok(())
}

#[test]
fn explicit_wrapper_and_internal_fonts_and_colors_remain_independent() -> Result {
    let mut source = native()?;
    let label = TextStyle {
        family: "Helvetica".into(),
        size_pt: 12.,
        bold: true,
        color: Color::Custom([124; 3]),
        ..Default::default()
    };
    let atom = TextStyle {
        family: "Arial".into(),
        size_pt: 9.,
        italic: true,
        color: Color::Custom([33, 65, 97]),
        ..Default::default()
    };
    let anchor = source.abbreviations[0].anchor;
    source.abbreviations[0].label_style = Some(label.clone());
    source.abbreviations[0].label_color_override = true;
    // The same 124 gray is intentional here, even on a white halo where
    // automatic contrast would choose another foreground.
    source.abbreviations[0].highlight = Some(Color::Custom([255; 3]));
    source.atom_mut(anchor).ok_or("O")?.text_style = Some(atom.clone());
    source.atom_mut(anchor).ok_or("O")?.display.color_override = true;
    let before = source.clone();
    for mut doc in std::iter::once(source.clone()).chain(editable_roundtrips(&source)?) {
        assert_structure(&doc);
        assert_editable_styles(&doc, &label, &atom, [255; 3])?;
        let group = doc.abbreviations.first().ok_or("OMe group")?;
        let anchor = group.anchor;
        assert_eq!(group.label_style.as_ref(), Some(&label));
        assert_eq!(
            doc.atom(anchor).ok_or("O")?.text_style.as_ref(),
            Some(&atom)
        );
        assert_eq!(
            canvas_theme::atom_color(&doc, doc.atom(anchor).ok_or("O")?),
            [124; 3]
        );
        assert_eq!(doc.expand_abbreviations(&[anchor]), 1);
        assert_eq!(
            canvas_theme::atom_color(&doc, doc.atom(anchor).ok_or("O")?),
            [33, 65, 97]
        );
        assert_eq!(
            doc.atom(anchor).ok_or("O")?.text_style.as_ref(),
            Some(&atom)
        );
    }
    assert_eq!(source, before);
    Ok(())
}
