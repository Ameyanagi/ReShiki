use super::*;
use crate::{
    document::Point,
    reactions::{Participant, Reaction},
};

fn molecule() -> Document {
    let mut doc = Document::default();
    let carbon = doc.add_atom("C", Point::default());
    let oxygen = doc.add_atom("O", Point::new(42., 0.));
    doc.add_bond(carbon, oxygen, 1, "plain");
    doc
}

#[tokio::test]
async fn copy_as_pictures_follow_the_editable_drawing_switch_and_background() {
    let engine = LocalEngine::in_process();
    for format in [CopyFormat::Png, CopyFormat::Svg, CopyFormat::Pdf] {
        for editable in [false, true] {
            let pictures = super::super::Pictures {
                background: export::Background::Color([0; 3]),
                dpi: 1200,
                editable,
            };
            let copy = prepare_as(engine.clone(), molecule(), format, pictures)
                .await
                .unwrap();
            let bytes = copy.representations[0].bytes().unwrap();
            assert_eq!(payload::read(&bytes).is_some(), editable, "{format:?}");
            if format == CopyFormat::Png {
                let raster = image::load_from_memory(&bytes).unwrap().into_rgba8();
                assert_eq!(raster.get_pixel(0, 0).0, [0, 0, 0, 255]);
            }
        }
    }
}

/// With the editable drawing off, the picture leaves it out, but ReShiki's own
/// clipboard data still pastes the drawing back on the desktop.
#[tokio::test]
async fn copy_as_pictures_paste_back_into_reshiki_without_the_editable_drawing() {
    let engine = LocalEngine::in_process();
    let pictures = super::super::Pictures {
        background: export::Background::Transparent,
        dpi: 1200,
        editable: false,
    };
    for format in [CopyFormat::Png, CopyFormat::Svg, CopyFormat::Pdf] {
        let copy = prepare_as(engine.clone(), molecule(), format, pictures)
            .await
            .unwrap();
        assert!(
            copy.representations
                .iter()
                .filter(|item| item.kind != super::super::NATIVE)
                .all(|item| payload::read(&item.bytes().unwrap()).is_none()),
            "{format:?} pictures leave the drawing out"
        );
        let pasted = super::super::paste_packet(
            engine.clone(),
            super::super::Packet {
                representations: copy.representations.clone(),
                generation: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(pasted.atoms.len(), 2, "{format:?} pastes as a drawing");
    }
}

#[tokio::test]
async fn copy_as_pictures_contain_the_selected_editable_drawing() {
    let mut source = molecule();
    source.version = 15;
    source.canvas_theme = crate::canvas_theme::CanvasTheme::Dark;
    source.bonds[0].color = crate::palette::Color::Custom([180, 50, 55]);
    let selected: Vec<_> = source.all_ids();
    let unselected = source.add_atom("N", Point::new(240., 0.));
    source.recent_colors.push([1, 2, 3]);
    let mut theme = crate::theme_files::bundled().unwrap().remove(0);
    theme.author = "Private author".into();
    theme.source = "Private source".into();
    theme.license = "Private license".into();
    source.custom_theme = Some(Box::new(theme));
    let source = selection_or_drawing(&source, &selected);
    let engine = LocalEngine::in_process();
    let (snapshot, _) = export::figure_document(&engine, source.clone())
        .await
        .unwrap();
    for (format, kind) in [
        (CopyFormat::Png, "public.png"),
        (CopyFormat::Svg, "public.svg-image"),
        (CopyFormat::Pdf, "com.adobe.pdf"),
    ] {
        let copy = prepare_as(engine.clone(), source.clone(), format, Default::default())
            .await
            .unwrap();
        let bytes = copy
            .representations
            .iter()
            .find(|item| item.kind == kind)
            .unwrap()
            .bytes()
            .unwrap();
        let restored = payload::read(&bytes).unwrap().unwrap();
        assert_eq!(restored.all_ids(), source.all_ids(), "{format:?}");
        assert!(restored.atom(unselected).is_none());
        assert_eq!(restored.bonds, source.bonds);
        assert_eq!(crate::scene::svg(&restored), crate::scene::svg(&snapshot));
        assert!(restored.recent_colors.is_empty());
        let theme = restored.custom_theme.unwrap();
        assert!(theme.author.is_empty() && theme.source.is_empty() && theme.license.is_empty());
        if format == CopyFormat::Svg {
            let svg = std::str::from_utf8(&bytes).unwrap();
            assert!(!svg.contains("<text"), "pasting apps need outlined labels");
            assert!(svg.contains("<path"));
        }
    }
}

#[test]
fn name_follows_inchi_in_copy_formats_and_uses_the_name_code() {
    let index = CopyFormat::ALL
        .iter()
        .position(|format| *format == CopyFormat::Inchi)
        .unwrap();
    assert_eq!(CopyFormat::ALL[index + 1], CopyFormat::Name);
    assert_eq!(CopyFormat::Name.label(), "Name");
    assert_eq!(CopyFormat::Name.code(), "name");
    assert!(CopyFormat::Name.is_chemical());
}

#[test]
fn name_needs_one_connected_molecule_without_a_reaction() {
    let format = CopyFormat::Name;
    assert!(format.unavailable_reason(&Document::default()).is_some());
    let mut doc = molecule();
    assert_eq!(format.unavailable_reason(&doc), None);
    let partial = selection_or_drawing(&doc, &[doc.atoms[0].id]);
    assert_eq!(format.unavailable_reason(&partial), None);
    assert_eq!(
        format.unavailable_reason_for_selection(&doc, &partial),
        Some(NAME_REASON)
    );
    let other = doc.add_atom("N", Point::new(240., 0.));
    assert_eq!(format.unavailable_reason(&doc), Some(NAME_REASON));
    doc.add_bond(doc.atoms[0].id, other, 1, "plain");
    doc.reactions.push(Reaction::new(99));
    assert!(format.unavailable_reason(&doc).is_some());
    doc.reactions.clear();
    assert_eq!(format.unavailable_reason(&doc), None);
}

#[tokio::test]
async fn name_copy_accepts_a_complete_drawn_salt() {
    let mut doc = Document::default();
    let sodium = doc.add_atom("Na", Point::default());
    let chloride = doc.add_atom("Cl", Point::new(80., 0.));
    doc.atom_mut(sodium).unwrap().charge = 1;
    doc.atom_mut(chloride).unwrap().charge = -1;
    assert_eq!(CopyFormat::Name.unavailable_reason(&doc), None);
    let copy = prepare_as(
        LocalEngine::default(),
        doc,
        CopyFormat::Name,
        Default::default(),
    )
    .await
    .unwrap();
    assert_eq!(copy, prepare_name("sodium chloride".into()));
}

#[test]
fn prepared_name_has_plain_html_and_rtf() {
    let name = "N1-ethyl-N2-methylethane-1,2-diamine";
    let copy = prepare_name(name.into());
    assert_eq!(copy.format, CopyFormat::Name);
    assert_eq!(copy.text(), Some(name));
    assert!(copy.notices.is_empty());
    assert_eq!(
        copy.representations
            .iter()
            .map(|r| r.kind.as_str())
            .collect::<Vec<_>>(),
        ["public.html", "public.rtf", TEXT]
    );
    assert_eq!(copy.representations[2].bytes().unwrap(), name.as_bytes());
    assert_eq!(
        String::from_utf8(copy.representations[0].bytes().unwrap()).unwrap(),
        "<meta charset=\"utf-8\"><i>N</i><sup>1</sup>-ethyl-<i>N</i><sup>2</sup>-methylethane-1,2-diamine"
    );
    let rtf = String::from_utf8(copy.representations[1].bytes().unwrap()).unwrap();
    assert!(rtf.contains("{\\i N}{\\super 1}-ethyl-{\\i N}{\\super 2}"));
}

#[test]
fn prepared_name_formats_isotope_masses_counts_and_roman_symbols() {
    for (name, html, rtf) in [
        (
            "(S)-(1-2H1,34S)ethan-1-ol",
            "(<i>S</i>)-(1-<sup>2</sup>H<sub>1</sub>,<sup>34</sup>S)ethan-1-ol",
            r"({\i S})-(1-{\super 2}H{\sub 1},{\super 34}S)ethan-1-ol",
        ),
        (
            "(31P)phosphane",
            "(<sup>31</sup>P)phosphane",
            r"({\super 31}P)phosphane",
        ),
        (
            "methanol-d4",
            "methanol-d<sub>4</sub>",
            r"methanol-d{\sub 4}",
        ),
    ] {
        let copy = prepare_name(name.into());
        assert_eq!(copy.text(), Some(name));
        assert_eq!(copy.representations[2].bytes().unwrap(), name.as_bytes());
        assert_eq!(
            String::from_utf8(copy.representations[0].bytes().unwrap()).unwrap(),
            format!("<meta charset=\"utf-8\">{html}"),
            "{name}"
        );
        assert_eq!(
            String::from_utf8(copy.representations[1].bytes().unwrap()).unwrap(),
            format!("{{\\rtf1\\ansi\\uc1 {rtf}}}"),
            "{name}"
        );
    }
}

#[tokio::test]
async fn name_preparation_generates_verified_ethanol() -> Result<(), String> {
    let mut doc = molecule();
    let carbon = doc.add_atom("C", Point::new(-42., 0.));
    doc.add_bond(carbon, doc.atoms[0].id, 1, "plain");
    let before = doc.clone();
    let copy = prepare_as(
        LocalEngine::default(),
        doc.clone(),
        CopyFormat::Name,
        Default::default(),
    )
    .await?;
    assert_eq!(copy, prepare_name("ethanol".into()));
    assert_eq!(doc, before);
    Ok(())
}

fn rings(arrow: bool) -> Document {
    let mut doc = Document::default();
    crate::editing::ring(&mut doc, Point::default(), 6, arrow, 36.);
    crate::editing::ring(&mut doc, Point::new(240., 0.), 6, false, 36.);
    if arrow {
        doc.arrows.push(crate::document::Arrow::new(
            doc.next_id(),
            Point::new(90., 0.),
            Point::new(150., 0.),
            Default::default(),
            Default::default(),
        ));
    }
    doc
}

#[tokio::test]
async fn chemical_text_copies_all_selected_disconnected_molecules_as_smiles() {
    let mut source = rings(false);
    let ids = source.all_ids();
    source.add_atom("N", Point::new(420., 0.));
    let before = source.clone();
    let snapshot = selection_or_drawing(&source, &ids);
    let (format, chemical) = chemical_text_snapshot(&source, &snapshot).unwrap();
    assert_eq!(format, CopyFormat::Smiles);
    assert_eq!(chemical, snapshot);
    let copy = prepare_chemical_text(Default::default(), chemical, format)
        .await
        .unwrap();
    let smiles = copy.text().unwrap();
    assert_eq!(smiles.split('.').count(), 2);
    let imported = LocalEngine::default()
        .request(crate::engine::Request::import_smiles(smiles))
        .await
        .unwrap()
        .document
        .unwrap();
    assert_eq!((imported.atoms.len(), imported.bonds.len()), (12, 12));
    assert_eq!(source, before);
    assert_eq!(
        copy.representations
            .iter()
            .map(|r| r.kind.as_str())
            .collect::<Vec<_>>(),
        ["org.opensmiles.smiles", TEXT]
    );
}

#[tokio::test]
async fn chemical_text_infers_aromatic_reaction_without_changing_drawing_roles() {
    let source = rings(true);
    let before = source.clone();
    let (format, chemical) = chemical_text_snapshot(&source, &source).unwrap();
    assert_eq!(format, CopyFormat::ChemDoodleReaction);
    assert_eq!(chemical.reactions.len(), 1);
    let copy = prepare_chemical_text(Default::default(), chemical, format)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_str(copy.text().unwrap()).unwrap();
    assert_eq!(json["m"].as_array().unwrap().len(), 2);
    assert_eq!(json["m"][0]["a"].as_array().unwrap().len(), 6);
    assert_eq!(json["m"][1]["a"].as_array().unwrap().len(), 6);
    assert_eq!(
        json["m"][0]["b"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|b| b["o"] == 2)
            .count(),
        3
    );
    assert_eq!(json["s"][0]["rs"].as_array().unwrap().len(), 1);
    assert_eq!(json["s"][0]["ps"].as_array().unwrap().len(), 1);
    assert_eq!(source, before);
    assert_eq!(copy.representations.len(), 1);
    assert_eq!(copy.representations[0].kind, TEXT);
}

#[tokio::test]
async fn chemical_text_matches_former_refresh_first_conversion() {
    for source in [rings(false), rings(true)] {
        let engine = LocalEngine::default();
        let (format, chemical) = chemical_text_snapshot(&source, &source).unwrap();
        let direct = prepare_chemical_text(engine.clone(), chemical, format)
            .await
            .unwrap();
        let reaction = crate::reactions::copy_reaction(&source, &source).unwrap();
        let mut refreshed = export::checked_document(&engine, source).await.unwrap();
        refreshed.reactions = reaction.into_iter().collect();
        let former = prepare_as(engine, refreshed, format, Default::default())
            .await
            .unwrap();
        assert_eq!(direct.text(), former.text());
        assert_eq!(direct.notices, former.notices);
    }
}

#[tokio::test]
async fn chemical_text_refreshes_hydrogen_cache_without_moving_the_authored_forward_arrow() {
    use crate::arrow_anchors::{Anchor, Target};
    let mut source = rings(true);
    let target = source.atoms[0].id;
    source.arrows[0].start_anchor = Some(Anchor {
        target: Target::Atom { atom: target },
        offset: Point::new(75., 0.),
        direction: Point::new(1., 0.),
        gap: 0.,
    });
    for atom in &mut source.atoms {
        atom.label_h = 0;
    }
    let before = source.clone();
    let engine = LocalEngine::in_process();
    let (format, chemical) = chemical_text_snapshot(&source, &source).unwrap();
    assert_eq!(format, CopyFormat::ChemDoodleReaction);
    let reaction = chemical.reactions[0].clone();
    let unrefreshed = prepare_as(engine.clone(), chemical.clone(), format, Default::default())
        .await
        .unwrap();
    let direct = prepare_chemical_text(engine.clone(), chemical, format)
        .await
        .unwrap();
    let mut refreshed = export::checked_document(&engine, source.clone())
        .await
        .unwrap();
    assert!(
        refreshed
            .atoms
            .iter()
            .zip(&source.atoms)
            .any(|(actual, original)| actual.label_h != original.label_h)
    );
    assert_eq!(refreshed.arrows, source.arrows);
    assert_eq!(refreshed.annotations, source.annotations);
    assert_eq!(refreshed.graphics, source.graphics);
    for (actual, original) in refreshed.atoms.iter().zip(&source.atoms) {
        let mut expected = original.clone();
        expected.label_h = actual.label_h;
        expected.cip_label.clone_from(&actual.cip_label);
        assert_eq!(actual, &expected);
    }
    refreshed.reactions = vec![reaction.clone()];
    let checked_copy = prepare_as(engine, refreshed, format, Default::default())
        .await
        .unwrap();
    assert_eq!(direct.text(), unrefreshed.text());
    assert_eq!(direct.text(), checked_copy.text());
    assert_eq!(direct.notices, checked_copy.notices);
    let json: serde_json::Value = serde_json::from_str(direct.text().unwrap()).unwrap();
    let arrow = &source.arrows[0];
    for (key, coordinate) in [
        ("x1", arrow.start.x),
        ("y1", arrow.start.y),
        ("x2", arrow.end.x),
        ("y2", arrow.end.y),
    ] {
        assert_eq!(json["s"][0][key].as_f64(), Some(f64::from(coordinate)));
    }
    assert_eq!(
        json["s"][0]["rs"],
        serde_json::json!([format!("a{}", reaction.reactants[0].atoms[0])])
    );
    assert_eq!(
        json["s"][0]["ps"],
        serde_json::json!([format!("a{}", reaction.products[0].atoms[0])])
    );
    assert_eq!(source, before);
}

#[tokio::test]
async fn chemical_text_preserves_explicit_source_roles_and_single_participant_copy() {
    let mut source = rings(true);
    let mut reaction = crate::reactions::copy_reaction(&source, &source)
        .unwrap()
        .unwrap();
    std::mem::swap(&mut reaction.reactants, &mut reaction.products);
    source.reactions = vec![reaction.clone()];
    let before = source.clone();
    let snapshot = selection_or_drawing(&source, &reaction.ids());
    let (format, chemical) = chemical_text_snapshot(&source, &snapshot).unwrap();
    assert_eq!(format, CopyFormat::ChemDoodleReaction);
    assert_eq!(chemical.reactions, source.reactions);
    let copy = prepare_chemical_text(Default::default(), chemical, format)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_str(copy.text().unwrap()).unwrap();
    assert_eq!(
        json["s"][0]["rs"],
        serde_json::json!([format!("a{}", reaction.reactants[0].atoms[0])])
    );
    assert_eq!(
        json["s"][0]["ps"],
        serde_json::json!([format!("a{}", reaction.products[0].atoms[0])])
    );
    let participant = selection_or_drawing(&source, &reaction.reactants[0].atoms);
    let (format, chemical) = chemical_text_snapshot(&source, &participant).unwrap();
    assert_eq!(format, CopyFormat::Smiles);
    assert_eq!(chemical.atoms.len(), 6);
    assert!(chemical.reactions.is_empty());
    assert!(
        prepare_chemical_text(Default::default(), chemical, format)
            .await
            .unwrap()
            .text()
            .is_some()
    );
    assert_eq!(source, before);
}

#[test]
fn chemical_text_rejects_ambiguous_reactions_and_pruned_explicit_roles() {
    let mut ambiguous = rings(true);
    for atom in &mut ambiguous.atoms[6..] {
        atom.position.x -= 120.;
    }
    assert!(
        chemical_text_snapshot(&ambiguous, &ambiguous)
            .unwrap_err()
            .contains("arrow midpoint")
    );
    let mut source = rings(true);
    let reaction = crate::reactions::copy_reaction(&source, &source)
        .unwrap()
        .unwrap();
    source.reactions = vec![reaction.clone()];
    let selected: Vec<_> = reaction.reactants[0]
        .atoms
        .iter()
        .copied()
        .chain([reaction.arrow])
        .collect();
    let incomplete = selection_or_drawing(&source, &selected);
    assert!(incomplete.reactions.is_empty());
    assert!(chemical_snapshot(&source, &incomplete).is_err());
    assert!(chemical_text_snapshot(&source, &incomplete).is_err());
    let without_arrow = selection_or_drawing(
        &source,
        &source.atoms.iter().map(|a| a.id).collect::<Vec<_>>(),
    );
    assert!(
        chemical_text_snapshot(&source, &without_arrow)
            .unwrap_err()
            .contains("participant")
    );
    let participant = selection_or_drawing(&source, &reaction.reactants[0].atoms);
    assert!(matches!(
        chemical_snapshot(&source, &participant).unwrap(),
        Cow::Borrowed(_)
    ));
}

#[tokio::test]
async fn dark_depth_copy_as_cdxml_and_cdx_preserves_exact_source_ink() {
    use crate::{canvas_theme::CanvasTheme, palette::Color};
    for frozen in [false, true] {
        let mut original = Document {
            canvas_theme: CanvasTheme::Dark,
            ..Document::default()
        };
        for (i, element) in ["O", "C", "O"].into_iter().enumerate() {
            let id = original.add_atom(element, Point::new(i as f32 * 42., 0.));
            original.atom_mut(id).unwrap().depth = i as f32 * 20. - 20.;
            if i > 0 {
                original.add_bond(id - 1, id, 1, "plain");
            }
        }
        let ids = original.all_ids();
        crate::depth_appearance::enable(&mut original, &ids, 0.6).unwrap();
        if frozen {
            crate::depth_appearance::freeze(&mut original, &ids);
        }
        let before = original.clone();
        for format in [CopyFormat::Cdxml, CopyFormat::Cdx] {
            let copy = prepare_as(
                Default::default(),
                original.clone(),
                format,
                Default::default(),
            )
            .await
            .unwrap();
            let xml = if format == CopyFormat::Cdxml {
                copy.text().unwrap().to_owned()
            } else {
                let representation = copy
                    .representations
                    .iter()
                    .find(|r| r.kind == CDX_TYPES[0])
                    .unwrap();
                crate::exchange::from_cdx(&representation.bytes().unwrap()).unwrap()
            };
            let restored = crate::chemistry::cdxml::import_cdxml(&xml)
                .unwrap()
                .document;
            let rear = restored
                .atoms
                .iter()
                .min_by(|a, b| a.position.x.total_cmp(&b.position.x))
                .unwrap();
            assert_eq!(
                rear.text_style.as_ref().unwrap().color.rgb(),
                [102; 3],
                "{format:?}/{frozen}"
            );
            let bond = restored
                .bonds
                .iter()
                .find(|b| b.a == rear.id || b.b == rear.id)
                .unwrap();
            assert_eq!(bond.color.rgb(), [140; 3], "{format:?}/{frozen}");
            assert!(restored.depth_appearance.is_empty());
            assert_eq!(original.depth_appearance, before.depth_appearance);
            assert_eq!(original.bonds[0].color, Color::Ink);
            assert_eq!(original, before);
        }
    }
}

#[tokio::test]
async fn bracketed_ion_copy_keeps_hidden_charge_data_and_grouped_superscript() {
    let mut document = molecule();
    let oxygen = document
        .atoms
        .iter()
        .find(|atom| atom.element == "O")
        .unwrap()
        .id;
    document.atom_mut(oxygen).unwrap().charge = -1;
    for atom in &mut document.atoms {
        atom.display.hide_charge = true;
    }
    let mut bracket = crate::graphics::Graphic::dragged(
        document.next_id(),
        crate::graphics::GraphicKind::Brackets,
        Point::new(-20., -25.),
        Point::new(65., 25.),
        Default::default(),
        crate::graphics::BracketSides::Both,
        false,
    );
    bracket.superscript = Some("−".into());
    document.graphics.push(bracket);
    document.group_selection(&document.all_ids()).unwrap();
    let source = document.clone();
    for format in [CopyFormat::Cdxml, CopyFormat::Cdx] {
        let copy = prepare_as(
            Default::default(),
            document.clone(),
            format,
            Default::default(),
        )
        .await
        .unwrap();
        let xml = if format == CopyFormat::Cdxml {
            copy.text().unwrap().to_owned()
        } else {
            let representation = copy
                .representations
                .iter()
                .find(|r| r.kind == CDX_TYPES[0])
                .unwrap();
            crate::exchange::from_cdx(&representation.bytes().unwrap()).unwrap()
        };
        let restored = crate::chemistry::cdxml::import_cdxml(&xml)
            .unwrap()
            .document;
        let oxygen = restored
            .atoms
            .iter()
            .find(|atom| atom.element == "O")
            .unwrap();
        assert_eq!(oxygen.charge, -1);
        assert!(oxygen.display.hide_charge);
        let annotation = restored
            .annotations
            .iter()
            .find(|label| label.text == "−")
            .unwrap();
        assert!(
            restored
                .groups
                .iter()
                .any(|group| group.members.contains(&annotation.id))
        );
        assert_eq!(document, source);
    }
}

#[test]
fn selection_does_not_expand_to_other_molecules_or_replace_the_source() {
    let mut doc = molecule();
    let selected = doc.all_ids();
    doc.add_atom("N", Point::new(200., 0.));
    let before = doc.clone();
    let part = selection_or_drawing(&doc, &selected);
    assert_eq!(part.atoms.len(), 2);
    assert_eq!(part.bonds.len(), 1);
    assert_eq!(selection_or_drawing(&doc, &[]), before);
    assert_eq!(doc, before);
}

#[test]
fn reaction_formats_require_complete_explicit_roles_without_unselected_atoms() {
    let mut doc = molecule();
    assert!(reaction_reason(&doc).is_some());
    let product = doc.add_atom("O", Point::new(240., 0.));
    let arrow = doc.next_id();
    doc.arrows.push(crate::document::Arrow::new(
        arrow,
        Point::new(90., 0.),
        Point::new(170., 0.),
        Default::default(),
        Default::default(),
    ));
    let mut reaction = Reaction::new(arrow);
    reaction.reactants.push(Participant {
        atoms: vec![1, 2],
        coefficient: 1,
    });
    doc.reactions.push(reaction);
    assert!(reaction_reason(&doc).is_some());
    doc.reactions[0].products.push(Participant {
        atoms: vec![product],
        coefficient: 1,
    });
    assert!(reaction_reason(&doc).is_none());
    let complete = doc.reactions[0].ids();
    assert!(reaction_reason(&selection_or_drawing(&doc, &[1, 2])).is_some());
    doc.add_atom("N", Point::new(420., 0.));
    assert!(reaction_reason(&doc).is_some());
    assert!(reaction_reason(&selection_or_drawing(&doc, &complete)).is_none());
    assert!(CopyFormat::Mol.unavailable_reason(&doc).is_some());
}

#[test]
fn chosen_formats_have_native_types_and_exact_text_without_hidden_native_data() {
    for (format, kind) in [
        (CopyFormat::Png, "public.png"),
        (CopyFormat::Svg, "public.svg-image"),
        (CopyFormat::Pdf, "com.adobe.pdf"),
        (CopyFormat::Mol, "com.mdli.molfile"),
        (CopyFormat::Smiles, "org.opensmiles.smiles"),
        (CopyFormat::Cdxml, "chemical/x-cdxml"),
        (CopyFormat::Cdx, CDX_TYPES[0]),
    ] {
        let binary = matches!(format, CopyFormat::Png | CopyFormat::Pdf | CopyFormat::Cdx);
        let data = "exact payload\n";
        let copy = from_output(
            format,
            data.as_bytes().to_vec(),
            (!binary).then(|| data.into()),
            vec!["keep warning".into()],
        )
        .unwrap();
        assert!(copy.representations.iter().any(|r| r.kind == kind));
        assert!(
            copy.representations
                .iter()
                .all(|r| r.kind != super::super::NATIVE)
        );
        if format == CopyFormat::Cdx {
            assert_eq!(copy.representations.len(), CDX_TYPES.len());
            assert!(
                copy.representations.iter().all(|item| {
                    std::sync::Arc::ptr_eq(&copy.representations[0].data, &item.data)
                })
            );
        }
        assert_eq!(copy.text(), (!binary).then_some(data));
        assert_eq!(copy.notices, ["keep warning"]);
        for representation in copy.representations {
            assert_eq!(representation.bytes().unwrap(), data.as_bytes());
        }
    }
    for format in [
        CopyFormat::Inchi,
        CopyFormat::Rxn,
        CopyFormat::ReactionSmiles,
        CopyFormat::ChemDoodleReaction,
    ] {
        let copy = from_output(format, b"text".to_vec(), Some("text".into()), vec![]).unwrap();
        assert_eq!(copy.representations.len(), 1);
        assert_eq!(copy.representations[0].kind, TEXT);
    }
}

#[tokio::test]
async fn molecular_formats_export_selected_graph_and_invalid_conversion_has_no_payload() {
    let engine = LocalEngine::default();
    let doc = molecule();
    let before = doc.clone();
    let smiles = prepare_as(
        engine.clone(),
        doc.clone(),
        CopyFormat::Smiles,
        Default::default(),
    )
    .await
    .unwrap();
    assert_eq!(smiles.text(), Some("CO"));
    let mol = prepare_as(
        engine.clone(),
        doc.clone(),
        CopyFormat::Mol,
        Default::default(),
    )
    .await
    .unwrap();
    let imported = crate::chemistry::molfile::read(mol.text().unwrap()).unwrap();
    assert_eq!(imported.molecule.ids.len(), 2);
    assert_eq!(doc, before);
    assert!(
        prepare_as(engine.clone(), doc, CopyFormat::Rxn, Default::default())
            .await
            .is_err()
    );
    assert!(
        prepare_as(
            engine,
            Document::default(),
            CopyFormat::Mol,
            Default::default()
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn figure_payloads_keep_the_chosen_format_with_the_editable_drawing() {
    let mut drawing = Document::default();
    drawing.arrows.push(crate::document::Arrow::new(
        1,
        Point::default(),
        Point::new(90., 0.),
        Default::default(),
        Default::default(),
    ));
    for format in [CopyFormat::Png, CopyFormat::Svg, CopyFormat::Pdf] {
        let copy = prepare_as(
            Default::default(),
            drawing.clone(),
            format,
            Default::default(),
        )
        .await
        .unwrap();
        assert_eq!(copy.format, format);
        let bytes = copy.representations[0].bytes().unwrap();
        assert_eq!(
            payload::read(&bytes).unwrap().unwrap().all_ids(),
            drawing.all_ids()
        );
        match format {
            CopyFormat::Png => {
                let image = image::load_from_memory(&bytes).unwrap().into_rgba8();
                assert_eq!(image.get_pixel(0, 0)[3], 0);
                assert!(copy.notices.iter().any(|notice| notice.contains("dpi")));
                assert!(copy.text().is_none());
            }
            CopyFormat::Svg => {
                let svg = std::str::from_utf8(&bytes).unwrap();
                let tree = roxmltree::Document::parse(svg).unwrap();
                assert!(tree.root_element().has_tag_name("svg"));
                assert_eq!(copy.text(), Some(svg));
            }
            CopyFormat::Pdf => {
                assert!(bytes.starts_with(b"%PDF-"));
                assert!(copy.text().is_none());
            }
            _ => unreachable!(),
        }
        assert!(
            copy.representations
                .iter()
                .all(|rep| rep.kind != super::super::NATIVE)
        );
        assert_eq!(
            copy.representations.len(),
            if format == CopyFormat::Svg { 2 } else { 1 }
        );
    }
}

#[tokio::test]
async fn reaction_payload_retains_participants_and_export_warnings() {
    let mut doc = molecule();
    let product = doc.add_atom("O", Point::new(240., 0.));
    let arrow = doc.next_id();
    doc.arrows.push(crate::document::Arrow::new(
        arrow,
        Point::new(90., 0.),
        Point::new(170., 0.),
        Default::default(),
        Default::default(),
    ));
    let mut reaction = Reaction::new(arrow);
    reaction.reactants.push(Participant {
        atoms: vec![1, 2],
        coefficient: 1,
    });
    reaction.products.push(Participant {
        atoms: vec![product],
        coefficient: 1,
    });
    doc.reactions.push(reaction);
    let rxn = prepare_as(
        Default::default(),
        doc.clone(),
        CopyFormat::Rxn,
        Default::default(),
    )
    .await
    .unwrap();
    let roundtrip = crate::chemistry::reaction::read_rxn(rxn.text().unwrap()).unwrap();
    assert_eq!(roundtrip.reactants.len(), 1);
    assert_eq!(roundtrip.products.len(), 1);
    assert!(
        rxn.notices
            .iter()
            .any(|notice| notice == crate::chemistry::reaction::EXPORT_WARNING)
    );
    let smiles = prepare_as(
        Default::default(),
        doc,
        CopyFormat::ReactionSmiles,
        Default::default(),
    )
    .await
    .unwrap();
    assert_eq!(smiles.text(), Some("CO>>O"));
    assert_eq!(rxn.notices, smiles.notices);
}

/// A catalytic cycle's step copies as its own reaction: the step's first
/// intermediate and reagents, then the next intermediate and products.
#[tokio::test]
async fn a_cycle_step_copies_as_its_own_reaction_smiles() {
    let mut doc = Document::default();
    let mut molecule = |atoms: &[(&str, f32, f32)]| {
        let ids: Vec<u64> = atoms
            .iter()
            .map(|&(element, x, y)| {
                let id = doc.add_atom(element, Point::new(x, y));
                doc.atom_mut(id).unwrap().no_implicit = element != "C";
                id
            })
            .collect();
        for pair in ids.windows(2) {
            doc.add_bond(pair[0], pair[1], 1, "plain");
        }
        ids
    };
    let pd = molecule(&[("Pd", 0., -200.)]);
    let oa = molecule(&[("C", 150., 90.), ("Pd", 180., 90.), ("Br", 210., 90.)]);
    let tm = molecule(&[("C", -250., 90.), ("Pd", -220., 90.), ("C", -190., 90.)]);
    let mebr = molecule(&[("C", 260., -160.), ("Br", 290., -160.)]);
    let ids: Vec<u64> = [&pd, &oa, &tm].into_iter().flatten().copied().collect();
    let (cycle, _) = crate::cycles::make(&mut doc, &ids).unwrap();
    let ink = crate::scene::selection_bounds(&doc, &mebr).unwrap();
    crate::cycles::assign(
        &mut doc,
        &crate::cycles::Target {
            cycle,
            role: crate::cycles::Role::Reagent(0),
            part: mebr,
            ink,
        },
    );
    let engine = LocalEngine::in_process();
    let mut lines = vec![];
    for s in 0..3 {
        let (copy, _) = crate::cycles::steps::step(&doc, cycle, s).unwrap();
        let smiles = prepare_as(
            engine.clone(),
            copy,
            CopyFormat::ReactionSmiles,
            Default::default(),
        )
        .await
        .unwrap();
        lines.push(smiles.text().unwrap().to_owned());
    }
    assert_eq!(
        lines,
        [
            "CBr.[Pd]>>[CH3][Pd][Br]",
            "[CH3][Pd][Br]>>[CH3][Pd][CH3]",
            "[CH3][Pd][CH3]>>[Pd]"
        ]
    );
}

/// A scheme copied with a catalytic cycle's part selected too is still its
/// own reaction: the part stays out of it and the copy.
#[test]
fn a_scheme_copies_without_a_selected_cycle_part() {
    let mut doc = Document::default();
    let mut pair = |x: f32, y: f32| {
        let a = doc.add_atom("C", Point::new(x, y));
        let b = doc.add_atom("O", Point::new(x + 40., y));
        doc.add_bond(a, b, 1, "plain");
        vec![a, b]
    };
    let left = pair(-200., 0.);
    let right = pair(200., 0.);
    let top = pair(0., -200.);
    let reactant = pair(600., 0.);
    let product = pair(900., 0.);
    let ids: Vec<u64> = [&left, &right, &top]
        .into_iter()
        .flatten()
        .copied()
        .collect();
    crate::cycles::make(&mut doc, &ids).unwrap();
    let arrow = doc.next_id();
    doc.arrows.push(crate::document::Arrow::new(
        arrow,
        Point::new(700., 20.),
        Point::new(860., 20.),
        crate::arrows::Preset::Forward,
        crate::arrows::ArrowStyle::preset(crate::arrows::Preset::Forward),
    ));
    let mut picked: Vec<u64> = [&top, &reactant, &product]
        .into_iter()
        .flatten()
        .copied()
        .collect();
    picked.push(arrow);
    let snapshot = crate::editing::selection(&doc, &picked);
    let chemical = chemical_snapshot(&doc, &snapshot).unwrap();
    assert!(top.iter().all(|id| chemical.atom(*id).is_none()));
    assert_eq!(chemical.reactions.len(), 1);
    assert_eq!(
        CopyFormat::ReactionSmiles.unavailable_reason(&chemical),
        None
    );
}
