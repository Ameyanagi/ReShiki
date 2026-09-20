use moruno::{
    assistant::{self, DrawingSettings, Molecule, Proposal, Step},
    document::{Document, Point},
    engine::{ChemistryEngine, PythonEngine, Request},
};
fn molecule(smiles: &str, label: &str) -> Molecule {
    Molecule {
        smiles: smiles.into(),
        label: label.into(),
        coefficient: 1,
        rotation: 0.,
    }
}
fn reaction() -> Proposal {
    Proposal {
        replace_ids: vec![],
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
        }],
    }
}
#[tokio::test]
async fn reaction_is_editable_validated_styled_and_exchangeable() {
    let engine = PythonEngine::default();
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
    assert_eq!(water_caption.position.y, product_caption.position.y);
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
    let engine = PythonEngine::default();
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
                moruno::typography::TextAlign::Center
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
    let engine = PythonEngine::default();
    let proposal = Proposal {
        replace_ids: vec![],
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
        }],
    };
    let engine = PythonEngine::default();
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
        moruno::atom_labels::HydrogenPosition::Left
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
    let picture = moruno::pictures::Picture::import(&pixels.into_inner()).unwrap();
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
    let engine = PythonEngine::default();
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
