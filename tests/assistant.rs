use reshiki::{
    assistant::{self, DrawingSettings, Molecule, Proposal, Step},
    document::{Document, Point},
    engine::{ChemistryEngine, LocalEngine, Request},
};
fn molecule(smiles: &str, label: &str) -> Molecule {
    Molecule {
        smiles: smiles.into(),
        label: label.into(),
        coefficient: 1,
        rotation: 0.,
        compact: false,
    }
}
fn reaction() -> Proposal {
    Proposal {
        replace_ids: vec![],
        composition: Default::default(),
        explanation: "Esterification".into(),
        molecules: vec![],
        reactions: vec![Step {
            reactants: vec![
                molecule("CC(=O)O", "Acetic acid"),
                molecule("CCO", "Ethanol"),
            ],
            products: vec![
                molecule("CCOC(C)=O", "Ethyl acetate"),
                molecule("O", "Water"),
            ],
            conditions: "H₂SO₄\nheat".into(),
            arrow: "forward".into(),
            title: String::new(),
            role: Default::default(),
            direction: None,
        }],
    }
}
#[tokio::test]
async fn reaction_is_editable_validated_styled_and_exchangeable() {
    let engine = LocalEngine::default();
    let mut settings = DrawingSettings {
        bond_length: 63.,
        bond_color: [40, 100, 140],
        ..Default::default()
    };
    settings.format.style.family = "Arial".into();
    settings.format.style.size_pt = 12.;
    settings.format.style.color = [90, 30, 130];
    let doc = assistant::render(&engine, &reaction(), &settings)
        .await
        .unwrap();
    assert_eq!(doc.arrows.len(), 1);
    assert_eq!(doc.reactions.len(), 1);
    assert_eq!(doc.reactions[0].reactants.len(), 2);
    assert_eq!(doc.reactions[0].products.len(), 2);
    assert_eq!(doc.reactions[0].arrow, doc.arrows[0].id);
    assert_eq!(doc.atoms.len(), 14);
    assert!(doc.bonds.iter().all(|b| b.color == settings.bond_color));
    assert!(
        doc.atoms
            .iter()
            .all(|a| a.text_style.as_ref() == Some(&settings.format.style))
    );
    assert!(doc.bonds.iter().all(|b| {
        (doc.atom(b.a)
            .unwrap()
            .position
            .distance(doc.atom(b.b).unwrap().position)
            - 63.)
            .abs()
            < 0.01
    }));
    let arrow = &doc.arrows[0];
    let conditions = doc
        .annotations
        .iter()
        .find(|a| a.text.contains("H₂SO₄"))
        .unwrap();
    assert!(conditions.position.y + conditions.size().1 < arrow.start.y);
    let water_caption = doc.annotations.iter().find(|a| a.text == "Water").unwrap();
    let product_caption = doc
        .annotations
        .iter()
        .find(|a| a.text == "Ethyl acetate")
        .unwrap();
    for (part, caption) in doc.reactions[0]
        .products
        .iter()
        .zip([product_caption, water_caption])
    {
        let (_, hi) = reshiki::scene::selection_bounds(&doc, &part.atoms).unwrap();
        assert!(caption.position.y > hi.y);
        assert!(caption.position.y - hi.y < settings.bond_length);
    }
    assert!(
        doc.annotations
            .iter()
            .all(|a| a.format.style.size_pt == 12.)
    );
    for format in ["cdxml", "cdx"] {
        let mut req = Request::molecule("export", doc.clone());
        req.format = Some(format.into());
        let output = engine.execute(req).await.unwrap().output.unwrap();
        let back = engine
            .execute(Request::import(format, &output))
            .await
            .unwrap()
            .document
            .unwrap();
        assert_eq!(back.atoms.len(), doc.atoms.len());
        assert_eq!(back.arrows.len(), 1);
    }
    let mut base = Document::default();
    let a = base.add_atom("C", Point::new(10., 10.));
    let (added, ids) = assistant::candidate(&base, &doc, &[]).unwrap();
    assert_eq!(base.atoms.len(), 1);
    assert_eq!(added.atom(a), base.atom(a));
    assert!(!ids.contains(&a));
    let (replaced, _) = assistant::candidate(&base, &doc, &[a]).unwrap();
    assert_eq!(replaced.atoms.len(), doc.atoms.len());
}

#[tokio::test]
async fn water_names_remain_captions_while_only_duplicate_formulas_are_hidden() {
    let engine = LocalEngine::default();
    for (label, coefficient, visible) in [
        ("Water", 1, true),
        ("water", 1, true),
        ("水", 1, true),
        ("H₂O (water)", 1, true),
        ("H2O", 1, false),
        ("H₂O", 1, false),
        ("3H2O", 3, false),
        ("3 H₂O", 3, false),
        // A different coefficient is not a redundant copy of the drawing.
        ("3 H₂O", 1, true),
    ] {
        let mut water = molecule("O", label);
        water.coefficient = coefficient;
        let proposal = Proposal {
            replace_ids: vec![],
            composition: Default::default(),
            explanation: String::new(),
            molecules: vec![water],
            reactions: vec![],
        };
        let doc = assistant::render(&engine, &proposal, &Default::default())
            .await
            .unwrap();
        assert_eq!(doc.atoms.len(), 1);
        let caption = doc.annotations.iter().find(|a| a.text == label);
        assert_eq!(caption.is_some(), visible, "{label}");
        if let Some(caption) = caption {
            assert!(caption.position.y > doc.atoms[0].position.y);
            assert_eq!(
                caption.format.alignment,
                reshiki::typography::TextAlign::Center
            );
        }
        if coefficient > 1 {
            assert!(
                doc.annotations
                    .iter()
                    .any(|a| a.text == coefficient.to_string())
            );
        }
    }
}
#[tokio::test]
async fn invalid_molecule_discards_entire_candidate_and_ambiguous_answer_has_no_drawing() {
    let engine = LocalEngine::default();
    let proposal = Proposal {
        replace_ids: vec![],
        composition: Default::default(),
        explanation: "Which isomer?".into(),
        molecules: vec![],
        reactions: vec![],
    };
    assert!(!proposal.has_drawing());
    assert!(
        assistant::render(&engine, &proposal, &Default::default())
            .await
            .unwrap()
            .atoms
            .is_empty()
    );
    let mut proposal = reaction();
    proposal.reactions[0]
        .products
        .push(molecule("NOT_A_SMILES", ""));
    assert!(
        assistant::render(&engine, &proposal, &Default::default())
            .await
            .is_err()
    );
    let mut proposal = reaction();
    proposal.reactions[0].products.clear();
    assert!(proposal.validate().is_err());
    assert!(
        serde_json::from_str::<Proposal>(
            r#"{"explanation":"x","molecules":[],"reactions":[],"command":"rm"}"#
        )
        .is_err()
    );
}

#[tokio::test]
async fn hydrolysis_uses_coefficients_and_editable_r_groups_without_overlapping_water() {
    let proposal = Proposal {
        replace_ids: vec![],
        composition: Default::default(),
        explanation: "General triglyceride hydrolysis".into(),
        molecules: vec![],
        reactions: vec![Step {
            reactants: vec![
                molecule(
                    "O=C([*:1])OCC(OC(=O)[*:2])COC(=O)[*:3]",
                    "トリアシルグリセロール",
                ),
                molecule("O.O.O", "3 H₂O"),
            ],
            products: vec![
                molecule("OCC(O)CO", "グリセリン"),
                molecule("[*:1]C(=O)O", ""),
                molecule("[*:2]C(=O)O", ""),
                molecule("[*:3]C(=O)O", ""),
            ],
            conditions: "酸触媒\n加熱".into(),
            arrow: "forward".into(),
            title: String::new(),
            role: Default::default(),
            direction: None,
        }],
    };
    let engine = LocalEngine::default();
    let doc = assistant::render(&engine, &proposal, &Default::default())
        .await
        .unwrap();
    assert_eq!(doc.abbreviations.len(), 6);
    let water: Vec<_> = doc
        .atoms
        .iter()
        .filter(|a| {
            a.element == "O"
                && a.label_h == 2
                && !doc.bonds.iter().any(|b| b.a == a.id || b.b == a.id)
        })
        .collect();
    assert_eq!(water.len(), 1);
    assert_eq!(
        water[0].display.hydrogen_position,
        reshiki::atom_labels::HydrogenPosition::Left
    );
    assert!(doc.annotations.iter().any(|a| a.text == "3"));
    assert!(!doc.annotations.iter().any(|a| a.text == "3 H₂O"));
    for format in ["cdxml", "cdx"] {
        let mut request = Request::molecule("export", doc.clone());
        request.format = Some(format.into());
        let output = engine.execute(request).await.unwrap().output.unwrap();
        let restored = engine
            .execute(Request::import(format, &output))
            .await
            .unwrap()
            .document
            .unwrap();
        assert_eq!(restored.abbreviations.len(), 6);
        assert_eq!(restored.atoms.len(), doc.atoms.len());
    }
    let mut invalid = proposal;
    invalid.reactions[0].reactants[1].coefficient = 3;
    assert!(
        assistant::render(&engine, &invalid, &Default::default())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn canvas_tools_return_live_data_and_images_without_mutating_the_document() {
    use assistant::canvas_tools::{CanvasTools, Snapshot};
    use base64::Engine;
    use std::sync::{Arc, RwLock};
    let mut document = Document::default();
    let atom = document.add_atom("O", Point::default());
    let mut pixels = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        40,
        30,
        image::Rgba([220, 80, 60, 255]),
    ))
    .write_to(&mut pixels, image::ImageFormat::Png)
    .unwrap();
    let picture = reshiki::pictures::Picture::import(&pixels.into_inner()).unwrap();
    document
        .graphics
        .push(picture.graphic(2, Point::new(100., 0.)));
    let canvas = Arc::new(RwLock::new(Snapshot {
        document: document.clone(),
        selected: vec![atom],
        revision: 8,
        epoch: 3,
    }));
    let tools = CanvasTools {
        canvas: canvas.clone(),
        settings: Default::default(),
        replace: vec![],
        revision: 8,
        epoch: 3,
    };
    let engine = LocalEngine::default();
    let inspect = tools
        .call("canvas_inspect", serde_json::json!({}), &engine)
        .await
        .unwrap();
    let text: serde_json::Value =
        serde_json::from_str(inspect["contentItems"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(text["selected_ids"], serde_json::json!([atom]));
    assert_eq!(text["revision"], 8);
    assert_eq!(
        text["document"]["graphics"][0]["picture"],
        serde_json::json!({"width_pixels":40,"height_pixels":30,"embedded":true})
    );
    assert!(!text.to_string().contains("iVBORw0KGgo"));
    let data = inspect["contentItems"][1]["imageUrl"]
        .as_str()
        .unwrap()
        .strip_prefix("data:image/png;base64,")
        .unwrap();
    let png = base64::engine::general_purpose::STANDARD
        .decode(data)
        .unwrap();
    assert!(png.starts_with(b"\x89PNG"));
    let rendered = image::load_from_memory(&png).unwrap().to_rgba8();
    assert!(rendered.pixels().any(|p| p.0 == [220, 80, 60, 255]));
    let preview = tools
        .call(
            "canvas_preview",
            serde_json::to_value(reaction()).unwrap(),
            &engine,
        )
        .await
        .unwrap();
    assert_eq!(preview["success"], true);
    assert_eq!(canvas.read().unwrap().document, document);
    let mut edit = reaction();
    edit.replace_ids = vec![atom];
    assert_eq!(tools.replacement(&edit).unwrap(), vec![atom]);
    edit.replace_ids = vec![99999];
    assert!(tools.replacement(&edit).is_err());
    edit.replace_ids = vec![atom];
    canvas.write().unwrap().revision = 9;
    assert!(tools.replacement(&edit).is_err());
    let inspect = tools
        .call("canvas_inspect", serde_json::json!({}), &engine)
        .await
        .unwrap();
    assert!(
        inspect["contentItems"][0]["text"]
            .as_str()
            .unwrap()
            .contains("\"revision\":9")
    );
    canvas.write().unwrap().epoch = 4;
    assert!(
        tools
            .call(
                "canvas_preview",
                serde_json::to_value(reaction()).unwrap(),
                &engine
            )
            .await
            .is_err()
    );
    assert_eq!(canvas.read().unwrap().document, document);
}

#[tokio::test]
async fn measured_compositions_preserve_bonds_and_separate_complete_panels() {
    use assistant::composition::{Arrangement, Role};
    let mut proposal = reaction();
    proposal.reactions.push(proposal.reactions[0].clone());
    proposal.reactions.push(proposal.reactions[0].clone());
    proposal.reactions[1].role = Role::Main;
    proposal.reactions[1].title = "General reaction".into();
    proposal.reactions[0].title = "Example A".into();
    proposal.reactions[2].title = "Example B".into();
    let engine = LocalEngine::default();
    for arrangement in [Arrangement::Rows, Arrangement::Central, Arrangement::Grid] {
        proposal.composition.arrangement = arrangement;
        proposal.composition.width_pt = 1000.;
        let doc = assistant::render(&engine, &proposal, &Default::default())
            .await
            .unwrap();
        let bounds: Vec<_> = doc
            .reactions
            .iter()
            .map(|r| reshiki::scene::selection_bounds(&doc, &r.ids()).unwrap())
            .collect();
        for (i, (lo, hi)) in bounds.iter().enumerate() {
            for (b, c) in bounds.iter().skip(i + 1) {
                assert!(
                    hi.x < b.x || c.x < lo.x || hi.y < b.y || c.y < lo.y,
                    "panels overlap: {arrangement:?}"
                );
            }
        }
        assert!(doc.bonds.iter().all(|b| {
            (doc.atom(b.a)
                .unwrap()
                .position
                .distance(doc.atom(b.b).unwrap().position)
                - 42.)
                .abs()
                < 0.01
        }));
        if arrangement == Arrangement::Rows {
            assert!(
                doc.arrows
                    .iter()
                    .all(|a| (a.start.x - doc.arrows[0].start.x).abs() < 0.01)
            );
        }
        if arrangement == Arrangement::Central {
            assert!(bounds[0].1.y < bounds[1].0.y && bounds[1].1.y < bounds[2].0.y);
        }
    }
}

#[tokio::test]
async fn compact_chains_are_expandable_without_losing_graph_or_stereo() {
    let engine = LocalEngine::default();
    let mut doc = engine
        .execute(Request::import_smiles("CCCCCCCC/C=C\\CCCCCCCC(=O)O"))
        .await
        .unwrap()
        .document
        .unwrap();
    let before = doc.clone();
    let count = assistant::composition::compact_chains(&mut doc, &before.all_ids()).unwrap();
    assert!(count > 0);
    assert_eq!(doc.atoms, before.atoms);
    assert_eq!(doc.bonds, before.bonds);
    assert!(doc.abbreviations.iter().all(|g| !g.label.contains("H0")));
    assert!(doc.expand_abbreviations(&before.all_ids()) > 0);
    assert_eq!(doc.atoms, before.atoms);
    assert_eq!(doc.bonds, before.bonds);
    let mut proposal = reaction();
    proposal.composition.preserve_details = true;
    proposal.reactions[0].reactants[0].compact = true;
    assert!(proposal.validate().is_err());
}

#[tokio::test]
async fn visual_edits_are_atomic_and_quality_checks_find_real_problems() {
    use assistant::review::{self, Edit};
    let doc = assistant::render(&LocalEngine::default(), &reaction(), &Default::default())
        .await
        .unwrap();
    assert!(
        review::quality(&doc, &Default::default()).is_empty(),
        "{:?}",
        review::quality(&doc, &Default::default())
    );
    let edits = [Edit::Move {
        target: "reaction:0".into(),
        dx_pt: 20.,
        dy_pt: 20.,
    }];
    let moved = review::apply(&doc, &edits, true).unwrap();
    assert_eq!(moved.bonds, doc.bonds);
    assert_eq!(moved.reactions, doc.reactions);
    assert_ne!(moved.atoms[0].position, doc.atoms[0].position);
    assert!(
        review::apply(
            &doc,
            &[
                edits[0].clone(),
                Edit::Move {
                    target: "caption:99999".into(),
                    dx_pt: 0.,
                    dy_pt: 0.
                }
            ],
            true
        )
        .is_err()
    );
    assert!(
        review::apply(
            &doc,
            &[Edit::Move {
                target: "reaction:0".into(),
                dx_pt: f32::NAN,
                dy_pt: 0.
            }],
            true
        )
        .is_err()
    );
    assert!(
        review::apply(
            &doc,
            &[Edit::Compact {
                target: "molecule:0".into()
            }],
            false
        )
        .is_err()
    );
    let mut bad = doc.clone();
    bad.annotations[1].position = bad.annotations[0].position;
    assert!(
        review::quality(&bad, &Default::default())
            .iter()
            .any(|s| s.contains("overlaps"))
    );
    bad.reactions[0].products[0].coefficient = 2;
    assert!(
        review::quality(&bad, &Default::default())
            .iter()
            .any(|s| s.contains("balanced"))
    );
    let ids = review::scope(&doc, &[doc.atoms[0].id]);
    assert!(doc.reactions[0].ids().iter().all(|id| ids.contains(id)));
    let images = review::images(&doc).unwrap();
    assert_eq!(images.len(), 2);
    assert!(images.iter().all(|(_, png)| png.starts_with(b"\x89PNG")));
}

#[tokio::test]
async fn branching_schemes_share_the_center_and_point_in_requested_directions() {
    use assistant::composition::Arrangement;
    let mut proposal = Proposal::default();
    proposal.composition.arrangement = Arrangement::Branching;
    for (direction, product) in [(0., "CC=O"), (-90., "C=C"), (180., "CCOC(C)=O")] {
        proposal.reactions.push(Step {
            reactants: vec![molecule("CCO", "Ethanol")],
            products: vec![molecule(product, "Product")],
            arrow: "forward".into(),
            conditions: "Reagents".into(),
            direction: Some(direction),
            ..Default::default()
        });
    }
    let doc = assistant::render(&LocalEngine::default(), &proposal, &Default::default())
        .await
        .unwrap();
    assert_eq!(doc.reactions.len(), 3);
    for r in &doc.reactions {
        assert_eq!(r.reactants[0].atoms, doc.reactions[0].reactants[0].atoms);
    }
    assert_eq!(
        doc.annotations
            .iter()
            .filter(|a| a.text == "Ethanol")
            .count(),
        1
    );
    for (arrow, step) in doc.arrows.iter().zip(&proposal.reactions) {
        let angle = (arrow.end.y - arrow.start.y)
            .atan2(arrow.end.x - arrow.start.x)
            .to_degrees();
        assert!(((angle - step.direction.unwrap() + 180.).rem_euclid(360.) - 180.).abs() < 0.001);
    }
    assert_eq!(doc.atoms.len(), 3 + 3 + 2 + 6);
    assert!(doc.bonds.iter().all(|b| {
        (doc.atom(b.a)
            .unwrap()
            .position
            .distance(doc.atom(b.b).unwrap().position)
            - 42.)
            .abs()
            < 0.01
    }));
    let images = assistant::review::images(&doc).unwrap();
    assert_eq!(images.len(), 4);
    let closeup = assistant::review::fragment(&doc, &doc.reactions[1].ids());
    for atom in &closeup.atoms {
        assert_eq!(Some(atom), doc.atom(atom.id));
    }
    assert!(
        closeup
            .atoms
            .iter()
            .any(|a| a.element == "O" && a.label_h > 0)
    );
    let scope = assistant::review::scope(&doc, &doc.reactions[2].products[0].atoms);
    assert!(
        doc.reactions
            .iter()
            .all(|r| r.ids().iter().all(|id| scope.contains(id)))
    );
    proposal.reactions[1].reactants[0].smiles = "CC".into();
    assert!(proposal.validate().is_err());
}

#[tokio::test]
async fn molecular_orientation_removes_small_tilts_without_changing_geometry() {
    let mut doc = LocalEngine::default()
        .execute(Request::import_smiles("CCCC(=O)OCC(OC(=O)CCC)COC(=O)CCC"))
        .await
        .unwrap()
        .document
        .unwrap();
    let ids = doc.all_ids();
    assistant::composition::straighten(&mut doc, &ids);
    reshiki::editing::transform(&mut doc, &ids, reshiki::editing::Transform::Rotate(7.25));
    let before = doc.clone();
    assert!(assistant::composition::straighten(&mut doc, &ids));
    assert_eq!(doc.bonds, before.bonds);
    assert_eq!(doc.atoms.len(), before.atoms.len());
    for bond in &doc.bonds {
        let a = doc.atom(bond.a).unwrap().position;
        let b = doc.atom(bond.b).unwrap().position;
        let angle = (b.y - a.y).atan2(b.x - a.x).to_degrees();
        assert!(
            (angle / 30. - (angle / 30.).round()).abs() < 0.001,
            "{angle}"
        );
        let old = before
            .atom(bond.a)
            .unwrap()
            .position
            .distance(before.atom(bond.b).unwrap().position);
        assert!((a.distance(b) - old).abs() < 0.001);
    }
    assert!(!assistant::composition::straighten(&mut doc, &ids));
    let mut proposal = reaction();
    proposal.reactions[0].reactants[0].rotation = 7.;
    assert!(proposal.validate().is_err());
    assert!(
        assistant::review::apply(
            &doc,
            &[assistant::review::Edit::Rotate {
                target: "molecule:0".into(),
                degrees: 7.
            }],
            true
        )
        .is_err()
    );
    let rotated = assistant::review::apply(
        &doc,
        &[assistant::review::Edit::Rotate {
            target: "molecule:0".into(),
            degrees: 90.,
        }],
        true,
    )
    .unwrap();
    assert_eq!(rotated.bonds, doc.bonds);
}
