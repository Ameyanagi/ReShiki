use super::*;
use crate::{
    document::Arrow,
    grouping::Group,
    molecule_names::MoleculeName,
    palette::Color,
    reactions::Reaction,
    scene::Primitive,
    typography::{TextAlign, TextFormat, TextSpan, TextStyle},
};

fn caption(doc: &mut Document, text: &str) -> u64 {
    let id = doc.next_id();
    doc.annotations.push(Annotation {
        id,
        position: Point::new(80., -35.),
        text: text.into(),
        format: TextFormat::default(),
    });
    id
}

fn point_eq(actual: Point, expected: Point) {
    assert!(
        actual.distance(expected) < 0.001,
        "{actual:?} != {expected:?}"
    );
}

#[test]
fn annotation_ink_bounds_match_world_scene_fragments() {
    let mut doc = Document::default();
    caption(&mut doc, "  Pd2+\nX‡  ");
    let annotation = &mut doc.annotations[0];
    annotation.format.alignment = TextAlign::Right;
    annotation.format.width_pt = Some(100.);
    annotation.format.style.formula = true;
    annotation.format.spans.push(TextSpan {
        start: 7,
        end: 11,
        style: TextStyle {
            size_pt: 18.,
            bold: true,
            italic: true,
            underline: true,
            script: Script::Superscript,
            ..TextStyle::default()
        },
    });
    doc.validate().unwrap();
    let drawing = crate::scene::primitives(&doc);
    let boxes = drawing.into_iter().flat_map(|primitive| match primitive {
        Primitive::Text {
            position,
            text,
            size,
            style,
            ..
        } => style::text_ink_boxes(&text, size, &style)
            .into_iter()
            .map(|(lo, hi)| (position.offset(lo.x, lo.y), position.offset(hi.x, hi.y)))
            .collect(),
        _ => vec![],
    });
    let (expected_lo, expected_hi) = ink_bounds(boxes).unwrap();
    let (lo, hi) = text_ink_bounds(&doc.annotations[0]).unwrap();
    point_eq(lo, expected_lo);
    point_eq(hi, expected_hi);
    assert!(lo.x > doc.annotations[0].position.x);
    assert!(hi.y > doc.annotations[0].position.y);
}

#[test]
fn first_character_position_matches_scene_with_alignment_and_scripts() {
    for alignment in [TextAlign::Left, TextAlign::Center, TextAlign::Right] {
        for (script, formula, underline) in [
            (Script::Normal, false, false),
            (Script::Normal, true, false),
            (Script::Subscript, false, false),
            (Script::Superscript, false, true),
        ] {
            let mut doc = Document::default();
            caption(&mut doc, "  Pd2+  ");
            let annotation = &mut doc.annotations[0];
            annotation.format.alignment = alignment;
            annotation.format.width_pt = Some(100.);
            annotation.format.style.script = script;
            annotation.format.style.formula = formula;
            annotation.format.style.underline = underline;
            let expected = crate::scene::primitives(&doc)
                .into_iter()
                .find_map(|primitive| {
                    let Primitive::Text {
                        position,
                        text,
                        size,
                        style,
                        ..
                    } = primitive
                    else {
                        return None;
                    };
                    let byte = text.find('P')?;
                    let advance = style::styled_text_width(&text[..byte], size, &style);
                    let (lo, hi) = ink_bounds(style::text_ink_boxes("P", size, &style))?;
                    Some(position.offset(advance + (lo.x + hi.x) / 2., (lo.y + hi.y) / 2.))
                })
                .unwrap();
            point_eq(
                text_first_glyph_center(&doc.annotations[0]).unwrap(),
                expected,
            );
        }
    }
    let mut doc = Document::default();
    caption(&mut doc, "   \n ");
    assert_eq!(text_ink_bounds(&doc.annotations[0]), None);
    assert_eq!(text_first_glyph_center(&doc.annotations[0]), None);
}

#[test]
fn plain_formula_and_manual_text_use_atom_entry_semantics() -> Result<(), String> {
    let raised = |start, end| ScriptRun {
        start,
        end,
        script: Script::Superscript,
    };
    let lowered = |start, end| ScriptRun {
        start,
        end,
        script: Script::Subscript,
    };
    for (text, formula, scripts, element, hydrogens, charge, variable, stored) in [
        ("Pd2+", false, vec![], "Pd", 0, 2, None, vec![]),
        ("Pd2+", false, vec![raised(2, 4)], "Pd", 0, 2, None, vec![]),
        ("Pd2+", true, vec![], "Pd", 0, 2, None, vec![]),
        (
            "M2+",
            false,
            vec![raised(1, 3)],
            "*",
            0,
            0,
            Some("M2+"),
            vec![raised(1, 3)],
        ),
        ("CH3", false, vec![], "C", 3, 0, None, vec![]),
        ("CH3", true, vec![lowered(2, 3)], "C", 3, 0, None, vec![]),
        ("NH256", false, vec![], "*", 0, 0, Some("NH256"), vec![]),
        (
            "  X‡  ",
            false,
            vec![raised(3, 6)],
            "*",
            0,
            0,
            Some("X‡"),
            vec![raised(1, 4)],
        ),
    ] {
        let mut source = Document::default();
        let existing = source.add_atom("N", Point::new(-40., 10.));
        let id = caption(&mut source, text);
        let annotation = &mut source.annotations[0];
        annotation.format.style.formula = formula;
        annotation.format.spans = scripts
            .iter()
            .map(|run| TextSpan {
                start: run.start,
                end: run.end,
                style: TextStyle {
                    script: run.script,
                    ..TextStyle::default()
                },
            })
            .collect();
        source.validate()?;
        let before = source.clone();
        assert_eq!(text_atom_refusal(&source, id), None, "{text}");
        let (converted, atom) = text_to_atom(&source, id)?;
        assert_eq!(source, before, "{text}");
        assert_ne!(atom, id);
        assert!(converted.annotations.is_empty(), "{text}");
        assert_eq!(converted.atom(existing), source.atom(existing), "{text}");
        let result = converted.atom(atom).ok_or("Missing converted atom")?;
        assert_eq!(result.element, element, "{text}");
        assert_eq!(result.explicit_h, hydrogens, "{text}");
        // Entry retains explicit H chemistry and clears the computed H cache.
        assert_eq!(result.label_h, 0, "{text}");
        assert_eq!(result.charge, charge, "{text}");
        assert_eq!(result.display.variable.as_deref(), variable, "{text}");
        assert_eq!(result.display.label_scripts, stored, "{text}");
        if hydrogens != 0 {
            assert!(result.no_implicit, "{text}");
            assert_eq!(crate::atom_text::entry(result), "CH3");
        }
        converted.validate()?;
    }
    Ok(())
}

#[test]
fn conversion_preserves_first_character_style_and_deletes_through_document() {
    let mut source = Document::default();
    let existing = source.add_atom("N", Point::new(-40., 10.));
    let id = caption(&mut source, "  R1  ");
    let other = caption(&mut source, "leave this caption");
    let first_style = TextStyle {
        family: "Arial".into(),
        size_pt: 18.,
        bold: true,
        italic: true,
        underline: true,
        color: Color::Custom([150, 35, 70]),
        script: Script::Superscript,
        formula: true,
    };
    source.annotations[0].format.spans.push(TextSpan {
        start: 2,
        end: 3,
        style: first_style.clone(),
    });
    source.groups.push(Group {
        id: source.next_id(),
        members: vec![existing, id, other],
        integral: false,
    });
    source.validate().unwrap();
    let first_glyph = text_first_glyph_center(&source.annotations[0]).unwrap();
    let before = source.clone();
    let (converted, atom) = text_to_atom(&source, id).unwrap();
    assert_eq!(source, before);
    let result = converted.atom(atom).unwrap();
    point_eq(result.position, text_atom_position(&source, id).unwrap());
    let (lo, hi) = crate::scene::atom_label_ink_boxes(result, &converted)[0];
    point_eq(
        Point::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.),
        first_glyph,
    );
    let mut expected_style = first_style;
    expected_style.script = Script::Normal;
    expected_style.formula = false;
    assert_eq!(result.text_style.as_ref(), Some(&expected_style));
    assert_eq!(converted.annotations, vec![source.annotations[1].clone()]);
    assert_eq!(converted.groups[0].members, vec![existing, other]);
    converted.validate().unwrap();
}

#[test]
fn invalid_text_and_non_text_ids_refuse_without_edits() {
    assert_eq!(
        TEXT_TOO_LONG,
        "Only a single line of up to 32 characters can become an atom label"
    );
    for text in [
        "".into(),
        "   ".into(),
        "X".repeat(40),
        "é".repeat(33),
        format!(" {} ", "é".repeat(31)),
        "Pd\n2+".into(),
        "Pd\r2+".into(),
        "Pd\t2+".into(),
        "Pd\u{0}2+".into(),
        "Pd\u{2028}2+".into(),
        "Pd\u{2029}2+".into(),
    ] {
        let mut source = Document::default();
        let id = caption(&mut source, &text);
        let before = source.clone();
        assert_eq!(
            text_atom_refusal(&source, id).as_deref(),
            Some(TEXT_TOO_LONG)
        );
        assert_eq!(text_to_atom(&source, id), Err(TEXT_TOO_LONG.into()));
        assert_eq!(source, before);
    }
    let mut source = Document::default();
    let atom = source.add_atom("C", Point::default());
    let before = source.clone();
    for id in [atom, atom + 1] {
        let reason = text_atom_refusal(&source, id).unwrap();
        assert_eq!(text_to_atom(&source, id), Err(reason));
        assert_eq!(source, before);
    }
}

#[test]
fn character_limit_counts_source_unicode_characters() {
    for (text, label) in [
        ("é".repeat(32), "é".repeat(32)),
        (format!(" {} ", "é".repeat(30)), "é".repeat(30)),
    ] {
        let mut source = Document::default();
        let id = caption(&mut source, &text);
        assert_eq!(text_atom_refusal(&source, id), None);
        let (converted, atom) = text_to_atom(&source, id).unwrap();
        assert_eq!(
            converted.atom(atom).unwrap().display.variable.as_ref(),
            Some(&label)
        );
    }
}

#[test]
fn molecule_names_and_reaction_conditions_stay_captions() {
    let mut named = Document::default();
    let atom = named.add_atom("C", Point::default());
    let id = caption(&mut named, "CH3");
    named.molecule_names.push(MoleculeName {
        annotation: id,
        atoms: vec![atom],
        smiles: "C".into(),
        text: "CH3".into(),
    });
    named.validate().unwrap();
    let mut reaction = Document::default();
    let condition = caption(&mut reaction, "Pd2+");
    let arrow = reaction.next_id();
    reaction.arrows.push(Arrow::new(
        arrow,
        Point::default(),
        Point::new(100., 0.),
        Default::default(),
        Default::default(),
    ));
    let mut membership = Reaction::new(arrow);
    membership.annotations.push(condition);
    reaction.reactions.push(membership);
    reaction.validate().unwrap();
    for (source, id, reason) in [
        (
            named,
            id,
            "This text is a molecule name; it stays a caption",
        ),
        (
            reaction,
            condition,
            "This text belongs to a reaction; it stays a caption",
        ),
    ] {
        let before = source.clone();
        let entries = crate::molecule_names::IDENTITY_ENTRIES.with(|entries| entries.get());
        assert_eq!(text_atom_refusal(&source, id).as_deref(), Some(reason));
        assert_eq!(text_to_atom(&source, id), Err(reason.into()));
        assert_eq!(source, before);
        assert_eq!(
            crate::molecule_names::IDENTITY_ENTRIES.with(|entries| entries.get()),
            entries
        );
    }
}

#[test]
fn admission_is_cheap_and_failed_candidates_are_atomic() {
    let mut source = Document::default();
    let id = caption(&mut source, "Pd2+");
    source.version = 0;
    let before = source.clone();
    assert_eq!(text_atom_refusal(&source, id), None);
    assert_eq!(text_to_atom(&source, id), Err(TEXT_TOO_LONG.into()));
    assert_eq!(source, before);
}
