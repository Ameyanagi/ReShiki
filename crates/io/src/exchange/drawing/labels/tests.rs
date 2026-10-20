use super::*;
use crate::chemistry::cdxml;
use crate::typography::ScriptRun;

fn dummy(label: &str) -> Document {
    let mut document = Document::default();
    let id = document.add_atom("*", Point::default());
    document
        .atom_mut(id)
        .unwrap()
        .display
        .set_variable(Some(label.into()));
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
    assert!(atom.display.label_scripts.is_empty());
}

#[test]
fn cdxml_dummy_labels_use_explicit_scripts_and_preserve_source_text() {
    for (label, expected) in [
        ("M2+", vec![("M", 0), ("2+", 64)]),
        ("R1", vec![("R", 0), ("1", 32)]),
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
    for label in ["M2+", "R1", "SO4 2-"] {
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
fn manual_dummy_scripts_remain_manual_in_cdxml_and_cdx() {
    for (label, end, script, expected) in [
        ("R1", 2, Script::Subscript, vec![("R", 0), ("1", 32)]),
        ("M2+", 3, Script::Superscript, vec![("M", 0), ("2+", 64)]),
        ("M2⁺", 5, Script::Superscript, vec![("M", 0), ("2⁺", 64)]),
    ] {
        let scripts = vec![ScriptRun {
            start: 1,
            end,
            script,
        }];
        let mut document = dummy(label);
        document.atoms.first_mut().unwrap().display.label_scripts = scripts.clone();
        let (xml, _) = super::super::write_clipboard(&document).unwrap();
        let decoded = crate::exchange::from_cdx(&crate::exchange::to_cdx(&xml).unwrap()).unwrap();
        for xml in [&xml, &decoded] {
            assert_eq!(
                runs(xml, "Unspecified"),
                expected
                    .iter()
                    .map(|(text, face)| ((*text).to_owned(), *face))
                    .collect::<Vec<_>>()
            );
            let tree = roxmltree::Document::parse(xml).unwrap();
            assert!(tree.descendants().any(|node| {
                node.has_tag_name("t")
                    && node.attribute("Name") == Some("ReShiki manual label scripts")
            }));
            let restored = import_variable(xml);
            let atom = restored.atoms.first().unwrap();
            assert_eq!(atom.display.variable.as_deref(), Some(label));
            assert_eq!(atom.display.label_scripts, scripts, "{label}");
        }
    }
}

#[test]
fn manual_normal_overrides_formula_faces_and_automatic_scripts() {
    for label in ["R1", "M2+", "OH"] {
        let mut document = dummy(label);
        let scripts = vec![ScriptRun {
            start: 0,
            end: label.len(),
            script: Script::Normal,
        }];
        let atom = document.atoms.first_mut().unwrap();
        atom.display.label_scripts = scripts.clone();
        atom.text_style = Some(TextStyle {
            formula: true,
            script: Script::Superscript,
            bold: true,
            ..Default::default()
        });
        let (xml, _) = super::super::write_clipboard(&document).unwrap();
        let decoded = crate::exchange::from_cdx(&crate::exchange::to_cdx(&xml).unwrap()).unwrap();
        for xml in [&xml, &decoded] {
            assert_eq!(runs(xml, "Unspecified"), vec![(label.into(), 1)]);
            let restored = import_variable(xml);
            let atom = restored.atoms.first().unwrap();
            assert_eq!(atom.display.variable.as_deref(), Some(label));
            assert_eq!(atom.display.label_scripts, scripts);
        }
    }
}

#[test]
fn handwritten_chemdraw_unspecified_scripts_import_as_byte_ranges() {
    for (source, label, scripts) in [
        (
            "<s face='0'>R</s><s face='64'>1</s>",
            "R1",
            vec![ScriptRun {
                start: 1,
                end: 2,
                script: Script::Superscript,
            }],
        ),
        (
            "<s face='32'>custom ligand</s>",
            "custom ligand",
            vec![ScriptRun {
                start: 0,
                end: 13,
                script: Script::Subscript,
            }],
        ),
        (
            "<s face='0'>Rα</s><s face='64'>β</s>",
            "Rαβ",
            vec![ScriptRun {
                start: 3,
                end: 5,
                script: Script::Superscript,
            }],
        ),
    ] {
        for element in [" Element='0'", ""] {
            let xml = format!(
                "<CDXML BondLength='30'><fonttable><font id='3' charset='utf-8' name='Arial'/></fonttable><page><fragment id='1'><n id='3' p='0 0' NodeType='Unspecified'{element}><t>{source}</t></n></fragment></page></CDXML>"
            );
            let decoded =
                crate::exchange::from_cdx(&crate::exchange::to_cdx(&xml).unwrap()).unwrap();
            for xml in [&xml, &decoded] {
                let restored = import_variable(xml);
                let atom = restored.atoms.first().unwrap();
                assert_eq!(atom.element, "*");
                assert_eq!(atom.display.variable.as_deref(), Some(label));
                assert_eq!(atom.display.label_scripts, scripts, "{source}");
            }
        }
    }
}

#[test]
fn chemdraw_formula_faces_keep_automatic_dummy_typography() {
    for label in ["R1", "M2+", "SO4 2-"] {
        let xml = format!(
            "<CDXML BondLength='30'><page><fragment id='1'><n id='3' p='0 0' NodeType='Unspecified' Element='0'><t><s face='96'>{label}</s></t></n></fragment></page></CDXML>"
        );
        let document = import_variable(&xml);
        let atom = document.atoms.first().unwrap();
        assert_eq!(atom.display.variable.as_deref(), Some(label));
        assert!(atom.display.label_scripts.is_empty());
        assert_eq!(
            crate::typography::effective_label_runs(&atom.display),
            crate::typography::label_runs(label)
        );
    }
}

#[test]
fn chemdraw_formula_beside_explicit_scripts_keeps_its_counts() {
    for (source, label, scripts) in [
        // Formula SO4 and a raised 2- read exactly as the automatic look.
        ("<s face='96'>SO4</s><s face='64'>2-</s>", "SO42-", vec![]),
        (
            "<s face='96'>Me3Si</s><s face='64'>‡</s>",
            "Me3Si‡",
            vec![
                ScriptRun {
                    start: 2,
                    end: 3,
                    script: Script::Subscript,
                },
                ScriptRun {
                    start: 5,
                    end: 8,
                    script: Script::Superscript,
                },
            ],
        ),
    ] {
        let xml = format!(
            "<CDXML BondLength='30'><fonttable><font id='3' charset='utf-8' name='Arial'/></fonttable><page><fragment id='1'><n id='3' p='0 0' NodeType='Unspecified' Element='0'><t>{source}</t></n></fragment></page></CDXML>"
        );
        let decoded = crate::exchange::from_cdx(&crate::exchange::to_cdx(&xml).unwrap()).unwrap();
        for xml in [&xml, &decoded] {
            let document = import_variable(xml);
            let atom = document.atoms.first().unwrap();
            assert_eq!(atom.display.variable.as_deref(), Some(label));
            assert_eq!(atom.display.label_scripts, scripts, "{source}");
            let lowered = crate::typography::effective_label_runs(&atom.display)
                .into_iter()
                .find(|(_, script)| *script == Script::Subscript)
                .map(|(range, _)| range);
            // The count stays lowered in both labels.
            assert_eq!(lowered, Some(2..3), "{source}");
        }
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
    document
        .atom_mut(id)
        .unwrap()
        .display
        .set_variable(Some("M2+".into()));
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

#[test]
fn text_label_justification_is_inherited_from_the_document() {
    use crate::atom_labels::LabelAttachment;
    for (root, text, expected) in [
        (" LabelJustification='Left'", "", LabelAttachment::Left),
        (
            " LabelJustification='Left'",
            " LabelJustification='Right'",
            LabelAttachment::Right,
        ),
        ("", "", LabelAttachment::Auto),
    ] {
        let xml = format!(
            "<CDXML BondLength='30'{root}><page><fragment id='1'><n id='3' p='0 0' NodeType='Unspecified' Element='0'><t{text}><s>MeO2C</s></t></n></fragment></page></CDXML>"
        );
        let document = import_variable(&xml);
        let atom = document.atoms.first().unwrap();
        assert_eq!(atom.display.label_attachment, expected, "{root}{text}");
    }
}

#[test]
fn flush_label_attachment_round_trips_as_chemdraw_justification() {
    use crate::atom_labels::LabelAttachment;
    for (attachment, justification, restored) in [
        (LabelAttachment::Left, Some("Left"), LabelAttachment::Left),
        (
            LabelAttachment::Right,
            Some("Right"),
            LabelAttachment::Right,
        ),
        (LabelAttachment::Auto, None, LabelAttachment::Auto),
        (
            LabelAttachment::Center,
            Some("Center"),
            LabelAttachment::Center,
        ),
        // A chosen character has no ChemDraw equivalent.
        (LabelAttachment::Character(1), None, LabelAttachment::Auto),
    ] {
        let mut document = dummy("MeO");
        let display = &mut document.atoms.first_mut().unwrap().display;
        display.label_attachment = attachment;
        // A ChemDraw import stores its LabelDisplay as the hydrogen side; a
        // later Automatic choice must not export that stale flush side.
        display.hydrogen_position = crate::atom_labels::HydrogenPosition::Left;
        let (xml, _) = super::super::write_clipboard(&document).unwrap();
        let decoded = crate::exchange::from_cdx(&crate::exchange::to_cdx(&xml).unwrap()).unwrap();
        for xml in [&xml, &decoded] {
            let tree = roxmltree::Document::parse(xml).unwrap();
            let text = tree
                .descendants()
                .find(|node| node.has_tag_name("t"))
                .unwrap();
            assert_eq!(text.attribute("LabelJustification"), justification);
            // ChemDraw 26 keeps the flush choice only with the node's
            // LabelDisplay; otherwise paste lays the label out automatically.
            assert_eq!(
                text.parent().unwrap().attribute("LabelDisplay"),
                Some(justification.unwrap_or("Auto"))
            );
            assert_eq!(text.attribute("Justification"), None);
            assert_eq!(text.attribute("LabelAlignment"), None);
            let restored_doc = import_variable(xml);
            let atom = restored_doc.atoms.first().unwrap();
            assert_eq!(atom.display.variable.as_deref(), Some("MeO"));
            assert_eq!(atom.display.label_attachment, restored, "{attachment}");
        }
    }
}

#[test]
fn label_justification_outranks_other_chemdraw_alignment_tags() {
    use crate::atom_labels::LabelAttachment;
    let mut document = dummy("MeO");
    document.atoms.first_mut().unwrap().display.label_attachment = LabelAttachment::Left;
    let (xml, _) = super::super::write_clipboard(&document).unwrap();
    let xml = xml
        .replace(
            "LabelJustification=\"Left\"",
            "LabelJustification=\"Left\" LabelAlignment=\"Right\"",
        )
        .replace("LabelDisplay=\"Left\"", "LabelDisplay=\"Right\"");
    assert!(xml.contains("LabelAlignment=\"Right\"") && xml.contains("LabelDisplay=\"Right\""));
    let atom = import_variable(&xml).atoms.first().unwrap().clone();
    assert_eq!(atom.display.label_attachment, LabelAttachment::Left);
}
