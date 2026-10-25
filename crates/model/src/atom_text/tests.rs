use super::*;
use crate::document::Point;

fn alpha_glucose() -> Result<Document, String> {
    let mut doc = crate::haworth::sugar_document(
        crate::haworth::Sugar::Glucose,
        crate::haworth::Anomer::Alpha,
        42.,
    )?;
    let ids = doc.atoms.iter().map(|atom| atom.id).collect::<Vec<_>>();
    doc.expand_abbreviations(&ids);
    Ok(doc)
}

fn depicted_smiles(text: &str) -> anyhow::Result<Document> {
    use crate::chemistry::{depict, document, smiles};
    let prepared = smiles::prepare(text)?;
    let state = prepared.state;
    let ranks = state
        .properties
        .atoms
        .iter()
        .map(|atom| atom.cip_rank)
        .collect::<Vec<_>>();
    let conformer = depict::compute(&state, &ranks, None, Default::default())?;
    let molecule = document::Molecule {
        rdkit_version: crate::chemistry::RDKIT_VERSION,
        ids: (1..=state.graph.atoms.len() as u64).collect(),
        positions: conformer.positions,
        state,
    };
    let drawing = document::for_import_scene(&molecule, false)?;
    let labels = drawing.labels()?;
    let doc = drawing.finish(labels)?;
    Ok(doc)
}

fn pentaerythritol() -> anyhow::Result<(Document, Vec<u64>)> {
    let doc = depicted_smiles("OCC(CO)(CO)CO")?;
    let mut oxygens = doc
        .atoms
        .iter()
        .filter(|atom| atom.element == "O")
        .map(|atom| (atom.id, atom.position))
        .collect::<Vec<_>>();
    oxygens.sort_by(|a, b| a.1.y.total_cmp(&b.1.y));
    oxygens[..2].sort_by(|a, b| a.1.x.total_cmp(&b.1.x));
    oxygens[2..].sort_by(|a, b| b.1.x.total_cmp(&a.1.x));
    Ok((doc, oxygens.into_iter().map(|(id, _)| id).collect()))
}

#[test]
fn durene_methoxy_repeats_keep_the_established_outward_readings() -> anyhow::Result<()> {
    let source = depicted_smiles("Cc1cc(C)c(C)cc1C")?;
    let mut ends = source
        .atoms
        .iter()
        .filter(|atom| {
            atom.element == "C"
                && source
                    .bonds
                    .iter()
                    .filter(|bond| bond.a == atom.id || bond.b == atom.id)
                    .count()
                    == 1
        })
        .map(|atom| (atom.id, atom.position))
        .collect::<Vec<_>>();
    ends.sort_by(|a, b| a.1.y.total_cmp(&b.1.y).then(a.1.x.total_cmp(&b.1.x)));
    assert_eq!(ends.len(), 4);
    ends.swap(2, 3);
    let mut product = source.clone();
    let mut three = None;
    for (index, &(id, position)) in ends.iter().enumerate() {
        let bond = source
            .bonds
            .iter()
            .find(|bond| bond.a == id || bond.b == id)
            .unwrap();
        let neighbor = source
            .atom(if bond.a == id { bond.b } else { bond.a })
            .unwrap();
        let left = neighbor.position.x > position.x + 0.1;
        product = apply(&product, id, "OMe", Mode::Auto).map_err(anyhow::Error::msg)?;
        assert_eq!(
            product.abbreviation(id).unwrap().text(&product),
            if left { "MeO" } else { "OMe" }
        );
        if index == 2 {
            three = Some(product.clone());
        }
    }
    assert_eq!(
        formula(&product).map_err(|error| anyhow::anyhow!("{error}"))?,
        "C10H14O4"
    );
    if let Some(directory) = std::env::var_os("RESHIKI_STEREO_REPEAT_QA_DIR") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory)?;
        for (name, doc) in [
            ("durene-input", source),
            ("durene-three-methoxy", three.unwrap()),
            ("durene-four-methoxy", product),
        ] {
            std::fs::write(
                directory.join(format!("{name}.rsk")),
                doc.file_json().map_err(anyhow::Error::msg)?,
            )?;
        }
        eprintln!("Durene methyl ends: {ends:?}");
    }
    Ok(())
}

#[test]
fn polyol_protection_faces_clear_ink_and_keeps_the_fourth_oxygen_targetable() -> anyhow::Result<()>
{
    let (source, oxygens) = pentaerythritol()?;
    let mut product = source.clone();
    for &oxygen in &oxygens[..3] {
        let refresh =
            crate::atom_labels::refresh::Refresh::calculate(&product, &Default::default())
                .map_err(anyhow::Error::msg)?;
        refresh.apply(&mut product);
        product = apply(&product, oxygen, "OAc", Mode::Auto).map_err(anyhow::Error::msg)?;
    }
    let last = *oxygens.last().unwrap();
    assert_eq!(
        crate::scene::atom_label_hit(&product, product.atom(last).unwrap().position, 0.),
        Some(last)
    );
    product = apply(&product, last, "OAc", Mode::Auto).map_err(anyhow::Error::msg)?;
    assert_eq!(
        formula(&product).map_err(|error| anyhow::anyhow!("{error}"))?,
        "C13H20O8"
    );
    let mut overlaps = Vec::new();
    for (index, a) in oxygens.iter().enumerate() {
        for b in &oxygens[index + 1..] {
            let a_ink = crate::scene::atom_label_ink_boxes(product.atom(*a).unwrap(), &product);
            let b_ink = crate::scene::atom_label_ink_boxes(product.atom(*b).unwrap(), &product);
            if a_ink.iter().any(|(lo, hi)| {
                b_ink
                    .iter()
                    .any(|(a, b)| hi.x > a.x && lo.x < b.x && hi.y > a.y && lo.y < b.y)
            }) {
                overlaps.push((*a, *b));
            }
        }
    }
    assert!(overlaps.is_empty(), "Overlapping acetate ink: {overlaps:?}");
    for &id in &oxygens {
        let ink = crate::scene::atom_label_ink_boxes(product.atom(id).unwrap(), &product);
        for bond in product
            .bonds
            .iter()
            .filter(|bond| bond.a != id && bond.b != id && product.bond_visible(bond.a, bond.b))
        {
            let a = product.atom(bond.a).unwrap().position;
            let b = product.atom(bond.b).unwrap().position;
            assert!(
                !ink.iter()
                    .any(|&(lo, hi)| crate::atom_labels::segment_hits_rect(a, b, lo, hi)),
                "Atom{id} ink crosses unrelated bond{}-{}",
                bond.a,
                bond.b
            );
        }
    }
    if let Some(directory) = std::env::var_os("RESHIKI_STEREO_REPEAT_QA_DIR") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory)?;
        std::fs::write(
            directory.join("pentaerythritol-input.rsk"),
            source.file_json().map_err(anyhow::Error::msg)?,
        )?;
        std::fs::write(
            directory.join("pentaerythritol-tetraacetate.rsk"),
            product.file_json().map_err(anyhow::Error::msg)?,
        )?;
        eprintln!("Polyol oxygen order: {oxygens:?}");
    }
    Ok(())
}

#[test]
fn polyol_retyping_keeps_explicit_alignment() -> anyhow::Result<()> {
    use crate::abbreviations::LabelAlignment;
    let (source, oxygens) = pentaerythritol()?;
    let id = oxygens[2];
    let product = apply(&source, id, "OAc", Mode::Auto).map_err(anyhow::Error::msg)?;
    for alignment in [
        LabelAlignment::Left,
        LabelAlignment::Right,
        LabelAlignment::Center,
        LabelAlignment::Above,
    ] {
        let mut placed = product.clone();
        placed
            .abbreviations
            .iter_mut()
            .find(|group| group.anchor == id)
            .unwrap()
            .alignment = alignment;
        let retyped = apply(&placed, id, "OBn", Mode::Auto).map_err(anyhow::Error::msg)?;
        assert_eq!(retyped.abbreviation(id).unwrap().alignment, alignment);
    }
    Ok(())
}

#[test]
#[ignore = "Opt-in actual-model inputs for native protection QA"]
fn glucose_protection_qa_inputs() -> Result<(), String> {
    let directory = std::path::PathBuf::from(
        std::env::var_os("RESHIKI_STEREO_REPEAT_QA_DIR")
            .ok_or("Set RESHIKI_STEREO_REPEAT_QA_DIR")?,
    );
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let source = alpha_glucose()?;
    std::fs::write(directory.join("glucose-input.rsk"), source.file_json()?)
        .map_err(|error| error.to_string())?;
    for label in ["OAc", "OBn"] {
        let product = apply(&source, 10, label, Mode::Auto)?;
        std::fs::write(
            directory.join(format!("glucose-c4-{label}.rsk")),
            product.file_json()?,
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn restore_reference_windings(template: &Document, product: &Document) -> Document {
    let mut expected = product.clone();
    for source in template.atoms.iter().filter(|atom| atom.stereo.is_some()) {
        expected.atom_mut(source.id).unwrap().stereo = source.stereo.clone();
    }
    expected
}

fn stereo_layers(state: &crate::chemistry::stereo::perception::State) -> anyhow::Result<String> {
    use crate::chemistry::inchi::kernel;
    let input = kernel::Molecule::prepare(state, None)?;
    // The pure InChI kernel owns a deeper recursion stack than a test thread.
    let generated = std::thread::Builder::new()
        .name("label-stereo-inchi-test".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || kernel::generate(&input))?
        .join()
        .map_err(|_| anyhow::anyhow!("InChI test worker panicked"))?
        .map_err(anyhow::Error::msg)?;
    anyhow::ensure!(!generated.inchi.is_empty(), "{}", generated.message);
    Ok(generated
        .inchi
        .split('/')
        .filter(|layer| matches!(layer.as_bytes().first(), Some(b't' | b'm' | b's')))
        .collect::<Vec<_>>()
        .join("/"))
}

fn assert_glucose_configuration(product: &Document, expected: &Document) -> anyhow::Result<()> {
    use crate::chemistry::{document, smiles};
    assert_eq!(
        product
            .atoms
            .iter()
            .filter(|atom| atom.stereo.is_some())
            .count(),
        5
    );
    let actual = document::prepare(product)?;
    let expected = document::prepare(expected)?;
    let expected_layers = stereo_layers(&expected.state)?;
    let actual_layers = stereo_layers(&actual.state)?;
    assert!(!expected_layers.contains('?'), "{expected_layers}");
    assert_eq!(actual_layers, expected_layers);
    let text = smiles::write::write(&actual.state, Default::default())?.text;
    assert_eq!(
        text.matches('@').count() - text.matches("@@").count(),
        5,
        "{text}"
    );
    let imported = smiles::prepare(&text)?;
    assert_eq!(stereo_layers(&imported.state)?, expected_layers, "{text}");
    let reopened = Document::from_json(&product.file_json().map_err(anyhow::Error::msg)?)
        .map_err(anyhow::Error::msg)?;
    let reopened = document::prepare(&reopened)?;
    assert_eq!(stereo_layers(&reopened.state)?, expected_layers);
    Ok(())
}

#[test]
fn terminal_glucose_protection_keeps_all_five_physical_configurations() -> anyhow::Result<()> {
    let template = alpha_glucose().map_err(anyhow::Error::msg)?;
    // Ring C1–C4 retain their OH oxygen IDs 7–10 when a group replaces them.
    for oxygen in 7..=10 {
        for label in ["OAc", "OBn", "MeO", "F", "SH"] {
            let product =
                apply(&template, oxygen, label, Mode::Auto).map_err(anyhow::Error::msg)?;
            let expected = restore_reference_windings(&template, &product);
            assert_glucose_configuration(&product, &expected)?;
        }
    }
    Ok(())
}

#[test]
fn full_glucose_protection_and_retyping_keep_all_configurations() -> anyhow::Result<()> {
    let template = alpha_glucose().map_err(anyhow::Error::msg)?;
    let mut product = template.clone();
    // The release example protects C6, C3, C2, C4, then the anomeric C1 oxygen.
    for (oxygen, label) in [(12, "OBn"), (9, "OBn"), (8, "OBn"), (10, "OBn"), (7, "MeO")] {
        product = apply(&product, oxygen, label, Mode::Auto).map_err(anyhow::Error::msg)?;
    }
    assert_glucose_configuration(&product, &restore_reference_windings(&template, &product))?;
    let acetyl = apply(&template, 10, "OAc", Mode::Auto).map_err(anyhow::Error::msg)?;
    let benzyl = apply(&acetyl, 10, "OBn", Mode::Auto).map_err(anyhow::Error::msg)?;
    assert_glucose_configuration(&benzyl, &restore_reference_windings(&template, &benzyl))?;
    Ok(())
}

fn chiral_alcohol() -> (Document, u64, u64) {
    let mut doc = Document::default();
    let center = doc.add_atom("C", Point::default());
    let fluorine = doc.add_atom("F", Point::new(-42., 0.));
    let chlorine = doc.add_atom("Cl", Point::new(21., -36.));
    let oxygen = doc.add_atom("O", Point::new(21., 36.));
    for neighbor in [fluorine, chlorine, oxygen] {
        doc.add_bond(center, neighbor, 1, "plain");
    }
    doc.atom_mut(center).unwrap().stereo = Some(crate::document::AtomStereo {
        winding: "cw".into(),
        neighbors: vec![fluorine, chlorine, oxygen],
    });
    (doc, center, oxygen)
}

#[test]
fn label_edits_clear_nonstereogenic_centers_and_computed_priorities() -> Result<(), String> {
    let (mut source, center, oxygen) = chiral_alcohol();
    source.atom_mut(center).unwrap().cip_label = Some("old priority".into());
    source.atom_mut(center).unwrap().explicit_h = 1;
    source.atom_mut(center).unwrap().no_implicit = true;
    source.bonds[0].stereo_authoritative = true;
    source.bonds[0].projection = true;
    source.bonds[0].z_order = 7;
    source.bonds[0].color = crate::palette::Color::Custom([30, 80, 140]);
    let retained = apply(&source, oxygen, "Br", Mode::Auto)?;
    assert_eq!(
        retained.atom(center).unwrap().stereo,
        source.atom(center).unwrap().stereo
    );
    assert_eq!(retained.atom(center).unwrap().cip_label, None);
    let duplicate = apply(&source, oxygen, "F", Mode::Auto)?;
    assert!(
        duplicate.atom(center).unwrap().stereo.is_none(),
        "Two identical fluorine ligands remove the center"
    );
    let mut without_stereo = source.clone();
    without_stereo.atom_mut(center).unwrap().stereo = None;
    let pre_cleanup_candidate = apply(&without_stereo, oxygen, "F", Mode::Auto)?;
    assert_eq!(
        duplicate, pre_cleanup_candidate,
        "Cleanup may remove the tag, not its explicit H, implicit-H policy, bonds or drawing metadata"
    );
    let edited_center = apply(&source, center, "N", Mode::Auto)?;
    assert!(edited_center.atom(center).unwrap().stereo.is_none());
    let mut changed_order = source.clone();
    changed_order.add_bond(center, oxygen, 2, "plain");
    assert!(changed_order.atom(center).unwrap().stereo.is_none());
    let mut removed = source.clone();
    removed.delete(&[oxygen]);
    assert!(removed.atom(center).unwrap().stereo.is_none());
    Ok(())
}

#[test]
fn a_remote_ligand_edit_clears_a_center_that_loses_its_distinct_branches() -> Result<(), String> {
    let mut source = Document::default();
    let center = source.add_atom("C", Point::default());
    let left = source.add_atom("C", Point::new(-42., 0.));
    let right = source.add_atom("C", Point::new(42., 0.));
    let fluorine = source.add_atom("F", Point::new(0., -42.));
    let oxygen = source.add_atom("O", Point::new(-84., 0.));
    let other_f = source.add_atom("F", Point::new(84., 0.));
    for (a, b) in [
        (center, left),
        (center, right),
        (center, fluorine),
        (left, oxygen),
        (right, other_f),
    ] {
        source.add_bond(a, b, 1, "plain");
    }
    source.atom_mut(center).unwrap().stereo = Some(crate::document::AtomStereo {
        winding: "cw".into(),
        neighbors: vec![left, right, fluorine],
    });
    let product = apply(&source, oxygen, "F", Mode::Auto)?;
    assert!(
        product.atom(center).unwrap().stereo.is_none(),
        "CH2OH becomes a second CH2F branch"
    );
    Ok(())
}

#[test]
fn stereo_validation_is_scoped_and_failure_keeps_the_original_configuration() -> Result<(), String>
{
    let (mut source, center, oxygen) = chiral_alcohol();
    let bad = source.add_atom("C", Point::new(400., 0.));
    for index in 0..5 {
        let fluorine = source.add_atom("F", Point::new(440., index as f32 * 20.));
        source.add_bond(bad, fluorine, 1, "plain");
    }
    let generic = source.add_atom("C", Point::new(500., 0.));
    source = apply(&source, generic, "Cl,Br", Mode::Auto)?;
    let product = apply(&source, oxygen, "OAc", Mode::Auto)?;
    assert_eq!(
        product.atom(center).unwrap().stereo,
        source.atom(center).unwrap().stereo
    );
    assert_eq!(product.generic, source.generic);
    assert_eq!(product.atom(bad), source.atom(bad));

    // Make the invalid fragment part of the affected molecule. Failure must
    // reject this edit, rather than accepting it with silently missing stereo.
    let mut invalid = source.clone();
    invalid
        .bonds
        .push(crate::document::Bond::plain(2, bad, 1, "plain"));
    let original = invalid.clone();
    let failure = apply(&invalid, oxygen, "OBn", Mode::Auto).unwrap_err();
    assert!(
        failure.starts_with("Could not verify stereochemistry after this label edit:"),
        "{failure}"
    );
    assert_eq!(invalid, original);
    assert!(invalid.atom(center).unwrap().stereo.is_some());
    let (mut generic, center, oxygen) = chiral_alcohol();
    let winding = generic.atom_mut(center).unwrap().stereo.take();
    generic = apply(&generic, 2, "Cl,Br", Mode::Auto)?;
    generic.atom_mut(center).unwrap().stereo = winding;
    generic.validate()?;
    let before = generic.clone();
    let failure = apply(&generic, oxygen, "OBn", Mode::Auto).unwrap_err();
    assert!(
        failure.starts_with("Could not verify stereochemistry after this label edit:"),
        "{failure}"
    );
    assert!(failure.contains(crate::generic::NOTICE), "{failure}");
    assert_eq!(
        generic, before,
        "An inconclusive connected generic case must remain atomic"
    );
    Ok(())
}

#[test]
fn formatted_labels_are_trimmed_with_their_runs() -> Result<(), String> {
    let mut source = Document::default();
    let id = source.add_atom("*", Point::default());
    for mode in [Mode::Auto, Mode::Text] {
        let result = apply_with_scripts(
            &source,
            id,
            "  R1  ",
            &[
                ScriptRun {
                    start: 0,
                    end: 2,
                    script: Script::Superscript,
                },
                ScriptRun {
                    start: 3,
                    end: 5,
                    script: Script::Subscript,
                },
            ],
            mode,
        )?;
        let atom = result.atom(id).ok_or("Atom")?;
        assert_eq!(atom.display.variable.as_deref(), Some("R1"));
        assert_eq!(
            atom.display.label_scripts,
            vec![ScriptRun {
                start: 1,
                end: 2,
                script: Script::Subscript,
            }]
        );
    }
    Ok(())
}

#[test]
fn manual_scripts_preserve_dummy_text_and_disambiguate_chemical_counts() -> Result<(), String> {
    let run = |start, end, script| ScriptRun { start, end, script };
    use Script::{Normal as N, Subscript as B, Superscript as P};
    let mut source = Document::default();
    let id = source.add_atom("C", Point::default());
    for (text, scripts, mode, element, hydrogens, charge) in [
        ("Pd2+", vec![run(2, 4, P)], Mode::Auto, "Pd", 0, 2),
        ("Pd2-", vec![run(2, 4, P)], Mode::Auto, "Pd", 0, -2),
        ("Pd2+", vec![run(2, 4, P)], Mode::Text, "*", 0, 0),
        (
            "Pd2+",
            vec![run(2, 3, B), run(3, 4, P)],
            Mode::Auto,
            "*",
            0,
            0,
        ),
        ("M2+", vec![run(1, 3, P)], Mode::Auto, "*", 0, 0),
        ("M2+", vec![run(0, 3, N)], Mode::Auto, "*", 0, 0),
        ("X‡", vec![run(1, 4, P)], Mode::Auto, "*", 0, 0),
        (
            "Pd9+",
            vec![run(2, 3, B), run(3, 4, P)],
            Mode::Auto,
            "*",
            0,
            0,
        ),
        (
            "Pd₂⁺",
            vec![run(2, 5, B), run(5, 8, P)],
            Mode::Auto,
            "*",
            0,
            0,
        ),
        (
            "NH12+",
            vec![run(2, 4, B), run(4, 5, P)],
            Mode::Auto,
            "N",
            12,
            1,
        ),
        (
            "NH4²⁺",
            vec![run(2, 3, B), run(3, 8, P)],
            Mode::Auto,
            "N",
            4,
            2,
        ),
        (
            "NH4+",
            vec![run(2, 3, B), run(3, 4, P)],
            Mode::Auto,
            "N",
            4,
            1,
        ),
        (
            "NH42+",
            vec![run(2, 3, B), run(3, 5, P)],
            Mode::Auto,
            "N",
            4,
            2,
        ),
        (
            "NH42+",
            vec![run(2, 3, B), run(3, 5, P)],
            Mode::Text,
            "*",
            0,
            0,
        ),
        ("R1", vec![run(1, 2, P)], Mode::Auto, "*", 0, 0),
        ("R1", vec![run(0, 2, N)], Mode::Auto, "*", 0, 0),
        ("Pd2+", vec![run(0, 4, N)], Mode::Auto, "Pd", 0, 2),
        ("NH4+", vec![run(0, 4, N)], Mode::Auto, "N", 4, 1),
        ("Pd", vec![run(0, 2, N)], Mode::Auto, "Pd", 0, 0),
        (
            "NH42",
            vec![run(2, 3, B), run(3, 4, P)],
            Mode::Auto,
            "*",
            0,
            0,
        ),
        ("Boc", vec![run(1, 2, P)], Mode::Auto, "*", 0, 0),
        ("C,N", vec![run(0, 3, N)], Mode::Auto, "*", 0, 0),
        (
            "NH4-",
            vec![run(2, 3, B), run(3, 4, P)],
            Mode::Auto,
            "N",
            4,
            -1,
        ),
        (
            "NH42-",
            vec![run(2, 3, B), run(3, 5, P)],
            Mode::Auto,
            "N",
            4,
            -2,
        ),
        (
            "NH12-",
            vec![run(2, 4, B), run(4, 5, P)],
            Mode::Auto,
            "N",
            12,
            -1,
        ),
    ] {
        let result = apply_with_scripts(&source, id, text, &scripts, mode)?;
        let atom = result.atom(id).ok_or("Atom")?;
        assert_eq!(
            (atom.element.as_str(), atom.explicit_h, atom.charge),
            (element, hydrogens, charge),
            "{text}: {mode:?} {scripts:?}"
        );
        assert!(result.generic.is_empty(), "{text}");
        assert!(result.abbreviations.is_empty(), "{text}");
        assert_eq!(result.atoms.len(), source.atoms.len());
        assert_eq!(result.bonds, source.bonds);
        if element == "*" {
            assert_eq!(atom.display.variable.as_deref(), Some(text));
            assert_eq!(
                atom.display.label_scripts,
                crate::typography::normalize_label_scripts(text, &scripts)?
            );
        } else {
            assert!(atom.display.variable.is_none());
            assert!(atom.display.label_scripts.is_empty());
        }
        let graph =
            crate::chemistry::document::drawing_graph(&result).map_err(|e| e.to_string())?;
        let chemical_atom = graph.atoms.first().ok_or("Chemical atom")?;
        assert_eq!(chemical_atom.atomic_number == 0, element == "*", "{text}");
        assert_eq!(
            u32::from(chemical_atom.explicit_hydrogens),
            hydrogens,
            "{text}"
        );
        assert_eq!(i32::from(chemical_atom.charge), charge, "{text}");
    }
    Ok(())
}

#[test]
fn empty_manual_runs_and_group_mode_keep_ordinary_entry_semantics() -> Result<(), String> {
    let mut source = Document::default();
    let id = source.add_atom("C", Point::default());
    for (text, mode) in [
        ("Boc", Mode::Auto),
        ("C,N", Mode::Auto),
        ("  R1  ", Mode::Text),
    ] {
        assert_eq!(
            apply_with_scripts(&source, id, text, &[], mode)?,
            apply(&source, id, text, mode)?
        );
    }
    let invalid = ScriptRun {
        start: 100,
        end: 0,
        script: Script::Superscript,
    };
    assert_eq!(
        apply_with_scripts(&source, id, "Boc", &[invalid], Mode::Group)?,
        apply(&source, id, "Boc", Mode::Group)?
    );
    let manual = apply_with_scripts(
        &source,
        id,
        "R1",
        &[ScriptRun {
            start: 1,
            end: 2,
            script: Script::Superscript,
        }],
        Mode::Text,
    )?;
    for (text, mode) in [("R1", Mode::Text), ("Pd", Mode::Auto), ("Boc", Mode::Group)] {
        let edited = apply(&manual, id, text, mode)?;
        assert!(
            edited
                .atoms
                .iter()
                .all(|atom| atom.display.label_scripts.is_empty())
        );
    }
    assert!(
        apply_with_scripts(
            &source,
            id,
            "R₁",
            &[ScriptRun {
                start: 2,
                end: 4,
                script: Script::Subscript
            }],
            Mode::Text
        )
        .is_err()
    );
    assert_eq!(source.atom(id).ok_or("Original atom")?.element, "C");
    assert_eq!(
        apply_with_scripts(
            &source,
            id,
            "Pd\n",
            &[ScriptRun {
                start: 0,
                end: 3,
                script: Script::Normal
            }],
            Mode::Auto
        ),
        Err("Use a single line of printable text".into())
    );
    Ok(())
}

#[test]
fn typed_charges_follow_element_hydride_text_and_link_grammars() -> Result<(), String> {
    enum Expected {
        Element(&'static str, u32, i32, &'static str),
        Dummy,
        Link,
        Error(&'static str),
    }
    use Expected::{Dummy, Element, Error, Link};
    let mut source = Document::default();
    let left = source.add_atom("C", Point::default());
    let id = source.add_atom("C", Point::new(42., 0.));
    let right = source.add_atom("C", Point::new(84., 0.));
    source.add_bond(left, id, 1, "plain");
    source.add_bond(id, right, 1, "plain");
    for (text, expected) in [
        ("Pd2+", Element("Pd", 0, 2, "Pd2+")),
        ("Pd+2", Element("Pd", 0, 2, "Pd2+")),
        ("Fe3+", Element("Fe", 0, 3, "Fe3+")),
        ("Na+", Element("Na", 0, 1, "Na+")),
        ("Cl-", Element("Cl", 0, -1, "Cl-")),
        ("O2-", Element("O", 0, -2, "O2-")),
        ("Pd²⁺", Element("Pd", 0, 2, "Pd2+")),
        ("NH4+", Element("N", 4, 1, "NH4+")),
        ("OH-", Element("O", 1, -1, "OH-")),
        (
            "NH42+",
            Error("Write the charge after the sign, for example NH4+2"),
        ),
        ("Pd9+", Error("Enter a charge from -8 to +8")),
        ("SO4 2-", Dummy),
        ("CH2 1-3", Link),
        ("M2+", Dummy),
    ] {
        let result = apply(&source, id, text, Mode::Auto);
        if let Error(message) = expected {
            assert_eq!(result, Err(message.into()), "{text}");
            continue;
        }
        let result = result?;
        let atom = result.atom(id).ok_or("Missing atom")?;
        assert_eq!(result.bonds, source.bonds, "{text}");
        match expected {
            Element(element, hydrogens, charge, canonical) => {
                assert_eq!(atom.element, element, "{text}");
                assert_eq!(atom.charge, charge, "{text}");
                assert!(atom.no_implicit, "{text}");
                assert_eq!(atom.explicit_h, hydrogens, "{text}");
                // label_h is computed; the edit clears it until the next check.
                assert_eq!(atom.label_h, 0, "{text}");
                assert!(atom.display.variable.is_none(), "{text}");
                assert!(result.generic.links.is_empty(), "{text}");
                assert_eq!(entry(atom), canonical, "{text}");
                assert_eq!(
                    apply(&source, id, &entry(atom), Mode::Auto)?,
                    result,
                    "{text}"
                );
            }
            Dummy => {
                assert_eq!(atom.element, "*", "{text}");
                assert_eq!(atom.charge, 0, "{text}");
                assert_eq!(atom.display.variable.as_deref(), Some(text));
                assert!(result.generic.links.is_empty(), "{text}");
            }
            Link => assert_eq!(
                result.generic.links,
                vec![crate::generic::LinkNode {
                    atom: id,
                    min: 1,
                    max: 3,
                }]
            ),
            Error(_) => unreachable!(),
        }
    }
    Ok(())
}

#[test]
fn charged_hydrogen_and_negative_hydrides_reopen_as_the_same_atom() -> Result<(), String> {
    let mut source = Document::default();
    let left = source.add_atom("C", Point::default());
    let id = source.add_atom("C", Point::new(42., 0.));
    source.add_bond(left, id, 1, "plain");
    for (text, canonical) in [
        ("H+", "H+"),
        ("H-", "H-"),
        ("NH4−2", "NH4-2"),
        ("CH2⁻³", "CH2-3"),
        ("NH12-1", "NH12-1"),
    ] {
        let typed = apply(&source, id, text, Mode::Auto)?;
        let atom = typed.atom(id).ok_or("Missing atom")?;
        assert!(atom.no_implicit, "{text}");
        assert!(typed.generic.links.is_empty(), "{text}");
        assert_eq!(entry(atom), canonical, "{text}");
        assert_eq!(apply(&source, id, canonical, Mode::Auto)?, typed, "{text}");
    }
    // Hydrogen keeps its bare entry when it has no typed charge.
    let neutral = apply(&source, id, "H", Mode::Auto)?;
    assert_eq!(entry(neutral.atom(id).ok_or("Missing H")?), "H");
    Ok(())
}

#[test]
fn charged_label_shapes_normalization_and_existing_errors() -> Result<(), String> {
    let mut source = Document::default();
    let id = source.add_atom("C", Point::default());
    for (text, element, hydrogens, charge) in [
        ("Co2+", "Co", 0, 2),
        ("Hf2+", "Hf", 0, 2),
        ("Hg+", "Hg", 0, 1),
        ("Pd₂⁺", "Pd", 0, 2),
        ("Pd⁺²", "Pd", 0, 2),
        ("O⁻²", "O", 0, -2),
        ("Cl−", "Cl", 0, -1),
        ("Pd+8", "Pd", 0, 8),
        ("O8-", "O", 0, -8),
        ("NH4+2", "N", 4, 2),
        ("NH12+1", "N", 12, 1),
        ("H3O+", "O", 3, 1),
        ("Hf", "Hf", 0, 0),
        ("He", "He", 0, 0),
        ("H+", "H", 0, 1),
        ("H-", "H", 0, -1),
        ("H⁺", "H", 0, 1),
        ("NH4−2", "N", 4, -2),
        ("NH4-2", "N", 4, -2),
        ("CH2-3", "C", 2, -3),
        ("NH12-1", "N", 12, -1),
    ] {
        let result = apply(&source, id, text, Mode::Auto)?;
        let atom = result.atom(id).ok_or("Missing atom")?;
        assert_eq!(atom.element, element, "{text}");
        assert_eq!(atom.explicit_h, hydrogens, "{text}");
        assert_eq!(atom.charge, charge, "{text}");
        assert_eq!(
            apply(&source, id, &entry(atom), Mode::Auto)?,
            result,
            "{text}"
        );
    }
    for text in [
        "Pd2+2", "Pd++", "Pd 2+", "Na+K", "A+B", "CN-", "*2+", "R-X", "ligand-1",
    ] {
        let result = apply(&source, id, text, Mode::Auto)?;
        let atom = result.atom(id).ok_or("Missing atom")?;
        assert_eq!(atom.element, "*", "{text}");
        assert_eq!(atom.display.variable.as_deref(), Some(text));
    }
    for text in [
        "Pd+9",
        "Pd0+",
        "Pd+0",
        "Pd⁰⁺",
        "C60+",
        "Pd999999999999999999999+",
        "Pd+999999999999999999999",
        "NH3+9",
    ] {
        assert_eq!(
            apply(&source, id, text, Mode::Auto),
            Err("Enter a charge from -8 to +8".into()),
            "{text}"
        );
    }
    for text in ["CH32+", "NH4²⁺", "NH12-"] {
        assert_eq!(
            apply(&source, id, text, Mode::Auto),
            Err("Write the charge after the sign, for example NH4+2".into()),
            "{text}"
        );
    }
    assert_eq!(
        apply(&source, id, "NH3+foo", Mode::Auto),
        Err("Use a charge such as +, - or +2".into())
    );
    assert_eq!(
        apply(&source, id, "NH256", Mode::Auto),
        Err("Hydrogen count must be at most 255".into())
    );
    Ok(())
}

#[test]
fn charged_elements_have_specific_guidance_and_text_mode_stays_literal() -> Result<(), String> {
    const CHARGED: &str = "An element with the typed charge. Hydrogens are not added; type the element alone to restore automatic hydrogens.";
    const HYDRIDE: &str = "An atom with the entered hydrogen count. Checking chemistry will not rewrite this label. Type the element alone to restore automatic hydrogens.";
    assert_eq!(description("Pd2+", &[], Mode::Auto), CHARGED);
    assert_eq!(description("NH4+", &[], Mode::Auto), HYDRIDE);
    assert_eq!(description("NH0+", &[], Mode::Auto), HYDRIDE);
    let mut source = Document::default();
    let id = source.add_atom("Pd", Point::default());
    let result = apply(&source, id, "Pd2+", Mode::Text)?;
    let atom = result.atom(id).ok_or("Missing atom")?;
    assert_eq!(atom.element, "*");
    assert_eq!(atom.charge, 0);
    assert_eq!(atom.display.variable.as_deref(), Some("Pd2+"));
    assert_eq!(
        description("Pd2+", &[], Mode::Text),
        "Text on a dummy atom; existing bonds stay connected."
    );
    Ok(())
}

#[test]
fn text_and_unrecognized_automatic_entries_show_scripts_without_assigning_chemistry()
-> Result<(), String> {
    let mut source = Document::default();
    let left = source.add_atom("C", Point::default());
    let id = source.add_atom("Pd", Point::new(42., 0.));
    let right = source.add_atom("C", Point::new(84., 0.));
    source.add_bond(left, id, 1, "plain");
    source.add_bond(id, right, 1, "plain");
    for (text, mode, core, suffix, superscript) in [
        ("Pd2+", Mode::Text, "Pd", "2+", true),
        ("M2+", Mode::Auto, "M", "2+", true),
        ("R1", Mode::Auto, "R", "1", false),
        ("L2", Mode::Auto, "L", "2", false),
    ] {
        let result = apply(&source, id, text, mode)?;
        let atom = result.atom(id).ok_or("Atom")?;
        assert_eq!(atom.element, "*", "{text}");
        assert_eq!(atom.charge, 0, "{text}");
        assert_eq!(atom.explicit_h, 0, "{text}");
        assert!(atom.no_implicit, "{text}");
        assert_eq!(atom.display.variable.as_deref(), Some(text));
        assert_eq!(
            atom.position,
            source.atom(id).ok_or("Original atom")?.position
        );
        assert_eq!(result.bonds, source.bonds, "{text}");
        assert!(result.abbreviations.is_empty(), "{text}");
        assert!(result.generic.is_empty(), "{text}");
        assert_eq!(entry(atom), text);
        let runs: Vec<_> = crate::scene::primitives(&result)
            .into_iter()
            .filter_map(|primitive| match primitive {
                crate::scene::Primitive::Text {
                    text,
                    size,
                    position,
                    ..
                } => Some((text, size, position)),
                _ => None,
            })
            .collect();
        assert_eq!(runs.len(), 2, "{text}: {runs:?}");
        let normal = runs.first().ok_or("Normal run")?;
        let script = runs.last().ok_or("Script run")?;
        assert_eq!(normal.0, core, "{text}");
        assert_eq!(script.0, suffix, "{text}");
        assert_eq!(normal.1, crate::style::DEFAULT.font_size(), "{text}");
        assert!(script.1 < normal.1, "{text}");
        assert_eq!(script.2.y < normal.2.y, superscript, "{text}");
        if !superscript {
            assert!(script.2.y > normal.2.y, "{text}");
        }
    }
    Ok(())
}

#[test]
fn charged_entry_preserves_implicit_hydrogens_and_imported_isotopes() -> Result<(), String> {
    for element in ["N", "Pt", "H"] {
        let mut source = Document::default();
        let id = source.add_atom(element, Point::default());
        let atom = source.atom_mut(id).ok_or("Missing atom")?;
        atom.charge = 1;
        assert_eq!(entry(atom), element);
        atom.no_implicit = true;
        atom.isotope = 2;
        let text = entry(atom);
        assert_eq!(text, format!("{element}+"));
        assert_eq!(apply(&source, id, &text, Mode::Auto)?, source);
        let atom = source.atom_mut(id).ok_or("Missing atom")?;
        atom.charge = 0;
        assert_eq!(
            entry(atom),
            if element == "H" {
                "H".into()
            } else {
                format!("{element}H0")
            }
        );
    }
    Ok(())
}

#[test]
fn free_labels_retain_connections_style_coordinates_and_roundtrip()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = Document::default();
    let metal = doc.add_atom("Cu", Point::new(0., 0.));
    let ligand = doc.add_atom("N", Point::new(60., 0.));
    doc.add_bond(ligand, metal, 5, "plain");
    let original = doc.clone();
    for label in ["M", "L", "X", "R₁", "custom ligand", "Boc"] {
        let mode = if label == "Boc" {
            Mode::Text
        } else {
            Mode::Auto
        };
        let named = apply(&doc, metal, label, mode)?;
        assert_eq!(named.bonds, original.bonds);
        assert_eq!(
            named.atom(metal).ok_or("Missing atom")?.position,
            Point::new(0., 0.)
        );
        assert_eq!(named.atom(metal).ok_or("Missing atom")?.element, "*");
        assert_eq!(
            named
                .atom(metal)
                .ok_or("Missing atom")?
                .display
                .variable
                .as_deref(),
            Some(label)
        );
        assert!(named.atom(metal).ok_or("Missing atom")?.no_implicit);
        // A label such as R₁ can span multiple font runs when the requested
        // face lacks a glyph. Check visible text, not contiguous XML bytes.
        let svg = crate::scene::svg(&named);
        let xml = roxmltree::Document::parse(&svg)?;
        let rendered: String = xml
            .descendants()
            .filter(|node| node.has_tag_name("text"))
            .filter_map(|node| node.text())
            .collect();
        assert!(rendered.contains(label), "Missing label {label}: {svg}");
        let saved = serde_json::to_string(&named)?;
        let loaded: Document = serde_json::from_str(&saved)?;
        loaded.validate()?;
        assert_eq!(loaded, named);
        let copy = crate::editing::selection(&named, &[metal, ligand]);
        assert_eq!(
            copy.atom(metal)
                .ok_or("Missing atom")?
                .display
                .variable
                .as_deref(),
            Some(label)
        );
        let restored = apply(&named, metal, "Fe", Mode::Auto)?;
        assert_eq!(restored.atom(metal).ok_or("Missing atom")?.element, "Fe");
        assert!(
            restored
                .atom(metal)
                .ok_or("Missing atom")?
                .display
                .variable
                .is_none()
        );
        assert_eq!(restored.bonds, original.bonds);
    }
    assert_eq!(doc, original);
    Ok(())
}

#[test]
fn boc_is_a_real_expandable_group_and_invalid_input_is_atomic()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = Document::default();
    let n = doc.add_atom("N", Point::new(0., 0.));
    let endpoint = doc.add_atom("C", Point::new(42., 0.));
    doc.add_bond(n, endpoint, 1, "plain");
    let before = doc.clone();
    let boc = apply(&doc, endpoint, "Boc", Mode::Auto)?;
    assert!(boc.atoms.len() > 2);
    let group = boc.abbreviation(endpoint).ok_or("Missing abbreviation")?;
    assert_eq!(group.label, "Boc");
    assert_eq!(
        boc.atom(endpoint).ok_or("Missing atom")?.position,
        doc.atom(endpoint).ok_or("Missing atom")?.position
    );
    assert!(boc.bonds.contains(doc.bonds.first().ok_or("Missing bond")?));
    crate::chemistry::document::prepare(&boc)?;
    let mut expanded = boc.clone();
    assert_eq!(expanded.expand_abbreviations(&[endpoint]), 1);
    assert_eq!(expanded.atoms, boc.atoms);
    assert_eq!(expanded.bonds, boc.bonds);
    assert!(apply(&boc, endpoint, "M", Mode::Auto).is_err());
    for label in ["", "  ", "X\nY", "abcdefghijklmnopqrstuvwxyz1234567"] {
        assert!(apply(&doc, endpoint, label, Mode::Text).is_err());
    }
    assert_eq!(doc, before);
    doc.add_bond(n, endpoint, 2, "plain");
    assert!(apply(&doc, endpoint, "Boc", Mode::Auto).is_err());
    assert!(apply(&doc, endpoint, "Boc", Mode::Text).is_ok());
    Ok(())
}

#[test]
fn lowered_counts_keep_a_known_group_chemical() -> Result<(), String> {
    let run = |start, end, script| ScriptRun { start, end, script };
    use Script::{Subscript as B, Superscript as P};
    let mut source = Document::default();
    let ring = source.add_atom("C", Point::default());
    let id = source.add_atom("C", Point::new(42., 0.));
    source.add_bond(ring, id, 1, "plain");
    for (text, runs) in [
        ("CO2Me", vec![run(2, 3, B)]),
        ("  CO2Me ", vec![run(4, 5, B)]),
        ("NO2", vec![run(2, 3, B)]),
        ("OC2H5", vec![run(2, 3, B), run(4, 5, B)]),
    ] {
        let result = apply_with_scripts(&source, id, text, &runs, Mode::Auto)?;
        assert_eq!(result, apply(&source, id, text, Mode::Auto)?, "{text}");
        assert!(result.abbreviation(id).is_some(), "{text}");
        assert!(result.atoms.len() > source.atoms.len(), "{text}");
    }
    // Any other script on a group name stays literal dummy text.
    for runs in [vec![run(0, 1, B)], vec![run(2, 3, P)], vec![run(2, 4, B)]] {
        let result = apply_with_scripts(&source, id, "CO2Me", &runs, Mode::Auto)?;
        assert_eq!(result.atom(id).ok_or("Atom")?.element, "*", "{runs:?}");
        assert!(result.abbreviations.is_empty(), "{runs:?}");
    }
    Ok(())
}

#[test]
fn finite_repeat_labels_are_described_as_repeats_not_dummy_text() {
    const REPEAT: &str = "A repeat of one atom over the typed count range. Expand generic structures generates each count.";
    for label in ["[CH2]1-3", "CH2 1-3", "(O)2"] {
        assert_eq!(description(label, &[], Mode::Auto), REPEAT, "{label}");
    }
    assert_eq!(
        description("[CH2]1-3", &[], Mode::Text),
        "Text on a dummy atom; existing bonds stay connected."
    );
    // The helper promises a repeat exactly when Apply creates a link node.
    let mut doc = Document::default();
    let ids = [0.0, 30.0, 60.0].map(|x| doc.add_atom("C", Point::new(x, 0.0)));
    doc.add_bond(ids[0], ids[1], 1, "plain");
    doc.add_bond(ids[1], ids[2], 1, "plain");
    for label in [
        "[CH2]1-3", "CH2 1-3", "(O)2", "[R]1-2", "[CH2]n", "NH4-2", "[Et]1-3", "[OMe]1-3",
        "[Cp]1-3", "[]1-3",
    ] {
        let linked = apply(&doc, ids[1], label, Mode::Auto)
            .is_ok_and(|result| !result.generic.links.is_empty());
        assert_eq!(
            description(label, &[], Mode::Auto) == REPEAT,
            linked,
            "{label}"
        );
    }
    // Manually formatted text is applied as text, never as a link node.
    let lowered = [ScriptRun {
        start: 3,
        end: 4,
        script: Script::Subscript,
    }];
    assert!(
        apply_with_scripts(&doc, ids[1], "[CH2]1-3", &lowered, Mode::Auto)
            .is_ok_and(|result| result.generic.links.is_empty())
    );
    assert_ne!(description("[CH2]1-3", &lowered, Mode::Auto), REPEAT);
}

fn formula(doc: &Document) -> Result<String, Box<dyn std::error::Error>> {
    let molecule = crate::chemistry::document::prepare(doc)?;
    let facts: Vec<_> = molecule
        .state
        .graph
        .atoms
        .iter()
        .zip(&molecule.state.valences)
        .map(|(a, v)| crate::chemistry::AtomFacts {
            atomic_number: a.atomic_number,
            isotope: a.isotope,
            charge: a.charge,
            hydrogens: (u32::from(a.explicit_hydrogens) + v.implicit_hydrogens) as u8,
            radical_electrons: a.radical_electrons,
        })
        .collect();
    Ok(crate::chemistry::properties(&facts)?.formula)
}

#[test]
fn protecting_group_labels_are_real_groups_that_expand_and_contract()
-> Result<(), Box<dyn std::error::Error>> {
    // Formulas of the methyl compound CH3-R, written out independently.
    for (label, reverse, expected) in [
        ("OBn", "BnO", "C8H10O"),
        ("PMB", "", "C9H12O"),
        ("OPMB", "PMBO", "C9H12O2"),
        ("OTBS", "TBSO", "C7H18OSi"),
        ("TIPS", "", "C10H24Si"),
        ("OTIPS", "TIPSO", "C10H24OSi"),
        ("TES", "", "C7H18Si"),
        ("OTES", "TESO", "C7H18OSi"),
        ("Piv", "", "C6H12O"),
        ("OPiv", "PivO", "C6H12O2"),
        ("Lev", "", "C6H10O2"),
        ("OLev", "LevO", "C6H10O3"),
        ("Tol", "", "C8H10"),
        ("SPh", "PhS", "C7H8S"),
        ("STol", "TolS", "C8H10S"),
        ("SEt", "EtS", "C3H8S"),
        ("NPhth", "PhthN", "C9H7NO2"),
        ("Troc", "", "C4H5Cl3O2"),
        ("NHTroc", "TrocHN", "C4H6Cl3NO2"),
        ("NHBoc", "BocHN", "C6H13NO2"),
        ("NHAc", "AcHN", "C3H7NO"),
        ("Tr", "", "C20H18"),
        ("OTr", "TrO", "C20H18O"),
        ("MOM", "", "C3H8O"),
        ("OMOM", "MOMO", "C3H8O2"),
        ("Tf", "", "C2H3F3O2S"),
        ("OTf", "TfO", "C2H3F3O3S"),
        ("All", "", "C4H8"),
        ("OAll", "AllO", "C4H8O"),
        ("CCl3", "Cl3C", "C2H3Cl3"),
        ("OC(NH)CCl3", "Cl3CC(NH)O", "C3H4Cl3NO"),
        ("N3", "", "CH3N3"),
    ] {
        let mut doc = Document::default();
        let methyl = doc.add_atom("C", Point::new(0., 0.));
        let endpoint = doc.add_atom("C", Point::new(42., 0.));
        doc.add_bond(methyl, endpoint, 1, "plain");
        let labelled = apply(&doc, endpoint, label, Mode::Auto)?;
        let group = labelled.abbreviation(endpoint).ok_or(label)?.clone();
        assert_eq!(
            (group.label.as_str(), group.reverse_label.as_str()),
            (label, reverse)
        );
        assert_eq!(formula(&labelled)?, expected, "{label}");

        let mut expanded = labelled.clone();
        assert_eq!(expanded.expand_abbreviations(&[endpoint]), 1);
        assert!(expanded.abbreviations.is_empty());
        assert_eq!(formula(&expanded)?, expected, "{label} expanded");
        if label == "N3" {
            continue; // A separately built group, not a detectable preset.
        }
        let molecule = crate::chemistry::document::prepare(&expanded)?;
        let contracted =
            crate::chemistry::abbreviations::find(&expanded, &molecule, &[], Some(label))?;
        let [found] = contracted.abbreviations.as_slice() else {
            panic!("{label} was not detected again after expansion");
        };
        assert_eq!(
            (found.label.as_str(), found.reverse_label.as_str()),
            (label, reverse)
        );
        // p-Tolyl on methyl is symmetric, so compare the group size, not IDs.
        assert_eq!(found.members.len(), group.members.len(), "{label}");
        assert_eq!(formula(&contracted)?, expected, "{label} contracted");
    }
    Ok(())
}

#[test]
fn catalog_aliases_create_the_same_group_under_the_typed_spelling()
-> Result<(), Box<dyn std::error::Error>> {
    for (alias, label) in [("TBDMS", "TBS"), ("MPM", "PMB"), ("Trt", "Tr")] {
        let mut doc = Document::default();
        let methyl = doc.add_atom("C", Point::new(0., 0.));
        let endpoint = doc.add_atom("C", Point::new(42., 0.));
        doc.add_bond(methyl, endpoint, 1, "plain");
        let typed = apply(&doc, endpoint, alias, Mode::Auto)?;
        let canonical = apply(&doc, endpoint, label, Mode::Auto)?;
        let group = typed.abbreviation(endpoint).ok_or(alias)?;
        assert_eq!(
            (group.label.as_str(), group.reverse_label.as_str()),
            (alias, "")
        );
        assert_eq!(formula(&typed)?, formula(&canonical)?, "{alias}");
        assert_eq!(typed.atoms.len(), canonical.atoms.len(), "{alias}");
    }
    Ok(())
}

#[test]
fn the_public_group_list_covers_every_typed_group() {
    let groups: Vec<_> = crate::abbreviations::groups().collect();
    let tbs = groups.iter().find(|g| g.label == "TBS").unwrap();
    assert_eq!(tbs.category, "silyl");
    assert_eq!(tbs.aliases, ["TBDMS"]);
    assert!(tbs.names.iter().any(|n| n == "tert-butyldimethylsilyl"));
    assert_eq!(tbs.formula, "C6H15Si");
    assert!(groups.iter().any(|g| g.label == "N3"));
    for label in crate::abbreviations::group_labels() {
        assert!(crate::abbreviations::is_group_label(&label), "{label}");
    }
    assert!(!crate::abbreviations::is_group_label("NotAGroup"));
}

/// Hill formula of CH3–R from the catalog's independently written formula
/// of R, so a typed group is checked against the hand-written composition.
fn methyl_formula(group: &str) -> String {
    let (body, charge) = match group.find(['+', '-']) {
        Some(index) => group.split_at(index),
        None => (group, ""),
    };
    let mut counts = std::collections::BTreeMap::<String, u32>::new();
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        let mut symbol = c.to_string();
        while let Some(&next) = chars.peek().filter(|n| n.is_ascii_lowercase()) {
            symbol.push(next);
            chars.next();
        }
        let mut digits = String::new();
        while let Some(&next) = chars.peek().filter(|n| n.is_ascii_digit()) {
            digits.push(next);
            chars.next();
        }
        *counts.entry(symbol).or_default() += digits.parse().unwrap_or(1);
    }
    *counts.entry("C".into()).or_default() += 1;
    *counts.entry("H".into()).or_default() += 3;
    let mut text = String::new();
    for symbol in ["C", "H"] {
        if let Some(count) = counts.remove(symbol) {
            text.push_str(symbol);
            if count > 1 {
                text.push_str(&count.to_string());
            }
        }
    }
    for (symbol, count) in counts {
        text.push_str(&symbol);
        if count > 1 {
            text.push_str(&count.to_string());
        }
    }
    text + charge
}

/// Every bundled label and alias, typed on the endpoint of CH3–C, creates
/// its group with the catalog formula. Groups that Contract may find are
/// found again after expansion. No spelling takes over an element or an
/// atom-with-hydrogens label such as NH2.
#[test]
fn every_bundled_spelling_types_as_its_group() -> Result<(), Box<dyn std::error::Error>> {
    let catalog = crate::chemistry::abbreviations::presets()?;
    let mut checked = 0;
    for preset in catalog.iter() {
        for spelling in preset.spellings() {
            // Acetyl and tosyl predate the catalog: "Ac" and "Ts" alone type
            // actinium and tennessine, so these groups need Chemical
            // abbreviation mode. No other spelling may shadow an element.
            let mode = if ["Ac", "Ts"].contains(&spelling) {
                Mode::Group
            } else {
                Mode::Auto
            };
            assert!(
                mode == Mode::Group || !ELEMENTS.contains(&spelling),
                "{spelling} is an element"
            );
            assert!(
                !matches!(hydride::parse(spelling), Ok(Some(_))),
                "{spelling} is an atom label"
            );
            assert!(
                description(spelling, &[], mode).starts_with("A real chemical group"),
                "{spelling}: {}",
                description(spelling, &[], mode)
            );
            let mut doc = Document::default();
            let methyl = doc.add_atom("C", Point::new(0., 0.));
            let endpoint = doc.add_atom("C", Point::new(42., 0.));
            doc.add_bond(methyl, endpoint, 1, "plain");
            let labelled =
                apply(&doc, endpoint, spelling, mode).map_err(|e| format!("{spelling}: {e}"))?;
            let group = labelled.abbreviation(endpoint).ok_or(spelling)?;
            assert_eq!(group.label, spelling);
            assert_eq!(group.members.len(), preset.atom_count(), "{spelling}");
            assert_eq!(
                formula(&labelled)?,
                methyl_formula(&preset.formula),
                "{spelling}"
            );
            checked += 1;
        }
        if !preset.contract {
            continue;
        }
        let mut doc = Document::default();
        let methyl = doc.add_atom("C", Point::new(0., 0.));
        let endpoint = doc.add_atom("C", Point::new(42., 0.));
        doc.add_bond(methyl, endpoint, 1, "plain");
        let mut expanded = apply(&doc, endpoint, &preset.label, Mode::Group)?;
        expanded.expand_abbreviations(&[endpoint]);
        let molecule = crate::chemistry::document::prepare(&expanded)?;
        let found =
            crate::chemistry::abbreviations::find(&expanded, &molecule, &[], Some(&preset.label))?;
        assert_eq!(
            found.abbreviations.len(),
            1,
            "{} not found again",
            preset.label
        );
    }
    assert!(checked > 350, "{checked}");
    Ok(())
}

/// CH3–C with the endpoint carbon's ID; the endpoint is the right-hand end.
fn ethane() -> (Document, u64) {
    ethane_with_end_at(42., 0.)
}

/// CH3–C with the endpoint carbon at `(x, y)` from the methyl.
fn ethane_with_end_at(x: f32, y: f32) -> (Document, u64) {
    let mut doc = Document::default();
    let methyl = doc.add_atom("C", Point::new(0., 0.));
    let endpoint = doc.add_atom("C", Point::new(x, y));
    doc.add_bond(methyl, endpoint, 1, "plain");
    (doc, endpoint)
}

#[test]
fn nc_and_ocn_are_reverse_spellings_only_where_they_read_toward_the_bond()
-> Result<(), Box<dyn std::error::Error>> {
    for (typed, label) in [("NC", "CN"), ("OCN", "NCO")] {
        // Left-hand end: the bond meets the last letter, as in NC–R.
        let (doc, endpoint) = ethane_with_end_at(-42., 0.);
        let left = apply(&doc, endpoint, typed, Mode::Auto)?;
        let group = left.abbreviation(endpoint).ok_or(typed)?;
        assert_eq!(
            (group.label.as_str(), group.reverse_label.as_str()),
            (label, typed)
        );
        assert!(!wrong_side(&doc, endpoint, typed));
        // Right-hand end (R–NC is an isocyanide) and a vertical bond stay text.
        for (x, y) in [(42., 0.), (0., 42.), (0., -42.)] {
            let (doc, endpoint) = ethane_with_end_at(x, y);
            assert!(wrong_side(&doc, endpoint, typed), "{typed} at {x},{y}");
            let text = apply(&doc, endpoint, typed, Mode::Auto)?;
            assert!(text.abbreviations.is_empty(), "{typed} at {x},{y}");
            let atom = text.atom(endpoint).ok_or(typed)?;
            assert_eq!(
                (atom.element.as_str(), atom.display.variable.as_deref()),
                ("*", Some(typed))
            );
            assert!(apply(&doc, endpoint, typed, Mode::Group).is_err());
            assert!(
                apply(&doc, endpoint, typed, Mode::Text)?
                    .abbreviations
                    .is_empty()
            );
        }
    }
    // Other reverse spellings have no second reading, so either end works.
    for (x, y) in [(-42., 0.), (42., 0.), (0., 42.)] {
        let (doc, endpoint) = ethane_with_end_at(x, y);
        assert!(!wrong_side(&doc, endpoint, "MeO"));
        let typed = apply(&doc, endpoint, "MeO", Mode::Auto)?;
        assert_eq!(
            typed.abbreviation(endpoint).map(|g| g.label.as_str()),
            Some("OMe")
        );
    }
    // The canonical labels are unaffected on any end.
    let (doc, endpoint) = ethane();
    assert!(!wrong_side(&doc, endpoint, "CN"));
    assert!(
        apply(&doc, endpoint, "CN", Mode::Auto)?
            .abbreviation(endpoint)
            .is_some()
    );
    Ok(())
}

#[test]
fn user_groups_spelled_nc_or_ocn_are_made_on_either_end() -> Result<(), Box<dyn std::error::Error>>
{
    let errors = crate::chemistry::abbreviations::set_user_on_thread(
        r#"{"format": 1, "groups": [
            {"label": "Isn", "reverse": "NC", "smiles": "*[N+]#[C-]"},
            {"label": "Cyt", "reverse": "OCN", "smiles": "*OC#N"}
        ]}"#,
    );
    assert!(errors.is_empty(), "{errors:?}");
    for (typed, label) in [("NC", "Isn"), ("OCN", "Cyt")] {
        for (x, y) in [(-42., 0.), (42., 0.), (0., 42.)] {
            let (doc, endpoint) = ethane_with_end_at(x, y);
            assert!(!wrong_side(&doc, endpoint, typed), "{typed} at {x},{y}");
            for mode in [Mode::Auto, Mode::Group] {
                let typed_doc = apply(&doc, endpoint, typed, mode)?;
                let group = typed_doc.abbreviation(endpoint).ok_or(typed)?;
                assert_eq!(group.label, label, "{typed} at {x},{y}");
            }
        }
    }
    Ok(())
}

#[test]
fn typed_reverse_spellings_make_their_group() -> Result<(), Box<dyn std::error::Error>> {
    for (typed, label) in [
        ("MeO", "OMe"),
        ("TBSO", "OTBS"),
        ("O2N", "NO2"),
        ("ON", "NO"),
        ("(HO)2B", "B(OH)2"),
        ("(EtO)3Si", "Si(OEt)3"),
    ] {
        let (doc, endpoint) = ethane();
        let reversed = apply(&doc, endpoint, typed, Mode::Auto)?;
        let canonical = apply(&doc, endpoint, label, Mode::Auto)?;
        let group = reversed.abbreviation(endpoint).ok_or(typed)?;
        assert_eq!(
            (group.label.as_str(), group.reverse_label.as_str()),
            (label, typed)
        );
        assert_eq!(formula(&reversed)?, formula(&canonical)?, "{typed}");
    }
    // A lowered count keeps a reverse spelling chemical, as it does a label.
    let (doc, endpoint) = ethane();
    let lowered = [ScriptRun {
        start: 3,
        end: 4,
        script: Script::Subscript,
    }];
    let typed = apply_with_scripts(&doc, endpoint, "MeO2C", &lowered, Mode::Auto)?;
    let group = typed.abbreviation(endpoint).ok_or("MeO2C")?;
    assert_eq!(
        (group.label.as_str(), group.reverse_label.as_str()),
        ("CO2Me", "MeO2C")
    );
    // Retyping the reverse spelling on that group keeps the same group.
    let retyped = apply(&typed, endpoint, "MeO2C", Mode::Auto)?;
    assert_eq!(formula(&retyped)?, formula(&typed)?);
    assert_eq!(
        retyped.abbreviation(endpoint).map(|g| g.label.as_str()),
        Some("CO2Me")
    );
    Ok(())
}

/// Every bundled reverse spelling, typed on C–CH3, creates its group under
/// the group's label, as the loop above does for labels and aliases.
#[test]
fn every_bundled_reverse_spelling_types_as_its_group() -> Result<(), Box<dyn std::error::Error>> {
    let catalog = crate::chemistry::abbreviations::presets()?;
    let mut checked = 0;
    for preset in catalog.iter().filter(|p| !p.user) {
        let reverse = preset.reverse_label.as_str();
        if reverse.is_empty() {
            continue;
        }
        assert!(!ELEMENTS.contains(&reverse), "{reverse} is an element");
        assert!(
            !matches!(hydride::parse(reverse), Ok(Some(_))),
            "{reverse} is an atom label"
        );
        assert!(
            description(reverse, &[], Mode::Auto).starts_with("A real chemical group"),
            "{reverse}: {}",
            description(reverse, &[], Mode::Auto)
        );
        // On a left-hand end, where a reverse spelling reads toward the bond.
        let (doc, endpoint) = ethane_with_end_at(-42., 0.);
        let labelled =
            apply(&doc, endpoint, reverse, Mode::Auto).map_err(|e| format!("{reverse}: {e}"))?;
        let group = labelled.abbreviation(endpoint).ok_or(reverse)?;
        assert_eq!(
            (group.label.as_str(), group.reverse_label.as_str()),
            (preset.label.as_str(), reverse)
        );
        assert_eq!(group.members.len(), preset.atom_count(), "{reverse}");
        assert_eq!(
            formula(&labelled)?,
            methyl_formula(&preset.formula),
            "{reverse}"
        );
        checked += 1;
    }
    assert_eq!(checked, 214);
    Ok(())
}

/// On a horizontal bond the replaced label already faces the same way as the
/// group, so the group keeps Auto alignment on either side.
#[test]
fn groups_typed_on_horizontal_bonds_stay_automatic() -> Result<(), Box<dyn std::error::Error>> {
    use crate::abbreviations::LabelAlignment;
    let (right_end, endpoint) = ethane();
    let mut left_end = Document::default();
    let start = left_end.add_atom("C", Point::new(0., 0.));
    let methyl = left_end.add_atom("C", Point::new(42., 0.));
    left_end.add_bond(start, methyl, 1, "plain");
    for (doc, id, drawn) in [(right_end, endpoint, "OMe"), (left_end, start, "MeO")] {
        for typed in ["OMe", "MeO"] {
            let result = apply(&doc, id, typed, Mode::Auto)?;
            let group = result.abbreviation(id).ok_or(typed)?;
            assert_eq!(
                (group.text(&result), group.alignment),
                (drawn, LabelAlignment::Auto)
            );
        }
    }
    Ok(())
}

#[test]
fn names_with_locants_never_reach_the_link_node_parser() -> Result<(), Box<dyn std::error::Error>> {
    for name in ["2-thienyl", "propan-2-yl", "4-methoxyphenyl"] {
        let (doc, endpoint) = ethane();
        let result = apply(&doc, endpoint, name, Mode::Auto)?;
        assert!(result.generic.links.is_empty(), "{name}");
        let atom = result.atom(endpoint).ok_or(name)?;
        assert_eq!(
            (atom.element.as_str(), atom.display.variable.as_deref()),
            ("*", Some(name))
        );
    }
    // Every catalog name, typed as written, is suggested and stays text.
    for name in crate::abbreviations::groups().flat_map(|g| g.names) {
        let (doc, endpoint) = ethane();
        let applied = apply(&doc, endpoint, &name, Mode::Auto);
        if name.chars().count() > 32 {
            assert_eq!(
                applied.err().as_deref(),
                Some("Enter 1–32 characters for the atom label")
            );
            continue;
        }
        let result = applied.map_err(|e| format!("{name}: {e}"))?;
        assert!(result.generic.links.is_empty(), "{name}");
        assert!(result.generic.alternatives.is_empty(), "{name}");
        assert!(suggestion(&name, Mode::Auto).is_some(), "{name}");
        assert!(
            description(&name, &[], Mode::Auto).starts_with("Text on a dummy atom"),
            "{name}: {}",
            description(&name, &[], Mode::Auto)
        );
    }
    Ok(())
}

#[test]
fn catalog_names_suggest_their_labels_without_making_the_group() -> Result<(), String> {
    assert_eq!(suggestions("morpholino", Mode::Auto), ["Morph"]);
    assert_eq!(suggestions("Acetyl", Mode::Auto), ["Ac", "COMe"]);
    assert!(suggestions("morpholino", Mode::Text).is_empty());
    assert!(suggestions("Morph", Mode::Auto).is_empty());
    assert!(
        description("OBn", &[], Mode::Auto).contains("Labels & chemistry → Chemical abbreviations")
    );
    assert_eq!(
        suggestion("morpholino", Mode::Auto).as_deref(),
        Some("Did you mean Morph?")
    );
    assert_eq!(
        suggestion("Acetyl", Mode::Auto).as_deref(),
        Some("Did you mean Ac or COMe?")
    );
    assert_eq!(suggestion("morpholino", Mode::Text), None);
    assert_eq!(suggestion("Morph", Mode::Auto), None);
    assert_eq!(suggestion("MOrph", Mode::Auto), None);
    let (doc, endpoint) = ethane();
    let typed = apply(&doc, endpoint, "morpholino", Mode::Auto)?;
    assert!(typed.abbreviations.is_empty());
    // Labels stay case-sensitive: MOrph is text, not the Morph group.
    let typed = apply(&doc, endpoint, "MOrph", Mode::Auto)?;
    assert!(typed.abbreviations.is_empty());
    assert_eq!(
        typed
            .atom(endpoint)
            .ok_or("atom")?
            .display
            .variable
            .as_deref(),
        Some("MOrph")
    );
    Ok(())
}

/// An `outside`-carbon chain on N(charge)–C(=O)–O–C(CH3)3.
fn carbamate(outside: usize, charge: i32) -> Document {
    let mut doc = Document::default();
    let mut previous: Option<u64> = None;
    let mut x = 0.;
    let mut chain = |doc: &mut Document, element: &str| {
        let id = doc.add_atom(element, Point::new(x, 0.));
        x += 42.;
        if let Some(previous) = previous {
            doc.add_bond(previous, id, 1, "plain");
        }
        previous = Some(id);
        id
    };
    for _ in 0..outside {
        chain(&mut doc, "C");
    }
    let nitrogen = chain(&mut doc, "N");
    let carbonyl = chain(&mut doc, "C");
    chain(&mut doc, "O");
    let quaternary = chain(&mut doc, "C");
    chain(&mut doc, "C");
    doc.atom_mut(nitrogen).unwrap().charge = charge;
    let oxygen = doc.add_atom("O", Point::new(0., 42.));
    doc.add_bond(carbonyl, oxygen, 2, "plain");
    for y in [-42., 42.] {
        let methyl = doc.add_atom("C", Point::new(0., y));
        doc.add_bond(quaternary, methyl, 1, "plain");
    }
    doc
}

fn contracted(doc: &Document) -> Result<Vec<(String, usize)>, Box<dyn std::error::Error>> {
    let molecule = crate::chemistry::document::prepare(doc)?;
    let found = crate::chemistry::abbreviations::find(doc, &molecule, &[], None)?;
    Ok(found
        .abbreviations
        .iter()
        .map(|group| (group.label.clone(), group.members.len()))
        .collect())
}

#[test]
fn contraction_falls_back_when_a_larger_group_cannot_apply()
-> Result<(), Box<dyn std::error::Error>> {
    // CNC(=O)OC(C)(C)C: NHBoc loses its outside methyl to Me, so Boc and Me
    // apply, as they did before NHBoc existed. Likewise with an ethyl.
    let expected = |a: usize, b: &str, c: usize| vec![("Boc".to_owned(), a), (b.to_owned(), c)];
    assert_eq!(contracted(&carbamate(1, 0))?, expected(7, "Me", 1));
    assert_eq!(contracted(&carbamate(2, 0))?, expected(7, "Et", 2));
    Ok(())
}

#[test]
fn contraction_never_hides_charge_or_isotopes() -> Result<(), Box<dyn std::error::Error>> {
    // C[N-]C(=O)OC(C)(C)C and its cation keep the charged nitrogen visible.
    for charge in [-1, 1] {
        let found = contracted(&carbamate(1, charge))?;
        assert!(found.iter().all(|(label, _)| label != "NHBoc"), "{found:?}");
    }
    // A 13C methyl is not an ordinary tert-butyl methyl.
    let mut doc = carbamate(1, 0);
    doc.atoms.last_mut().unwrap().isotope = 13;
    let found = contracted(&doc)?;
    assert!(
        found
            .iter()
            .all(|(label, _)| label != "Boc" && label != "NHBoc"),
        "{found:?}"
    );
    Ok(())
}

/// Cyclohexyl–X(Y)Z with X carrying `charge` and the given hydrogens.
fn cyclohexyl(center: &str, charge: i32, hydrogens: u32, arms: [(&str, i32); 2]) -> Document {
    let mut doc = Document::default();
    let ring: Vec<_> = (0..6)
        .map(|i| {
            let angle = i as f32 * std::f32::consts::FRAC_PI_3;
            doc.add_atom("C", Point::new(42. * angle.cos(), 42. * angle.sin()))
        })
        .collect();
    for (i, &atom) in ring.iter().enumerate() {
        doc.add_bond(atom, ring[(i + 1) % 6], 1, "plain");
    }
    let x = doc.add_atom(center, Point::new(84., 0.));
    doc.add_bond(ring[0], x, 1, "plain");
    let atom = doc.atom_mut(x).unwrap();
    atom.charge = charge;
    atom.explicit_h = hydrogens;
    for (index, (element, charge)) in arms.into_iter().enumerate() {
        let arm = doc.add_atom(element, Point::new(126., index as f32 * 42. - 21.));
        doc.atom_mut(arm).unwrap().charge = charge;
        doc.add_bond(x, arm, 1, "plain");
    }
    doc
}

#[test]
fn boronic_acids_and_phosphines_contract_only_when_neutral()
-> Result<(), Box<dyn std::error::Error>> {
    let labels = |doc: &Document| -> Result<Vec<String>, Box<dyn std::error::Error>> {
        Ok(contracted(doc)?
            .into_iter()
            .map(|(label, _)| label)
            .collect())
    };
    let boronic = cyclohexyl("B", 0, 0, [("O", 0), ("O", 0)]);
    assert!(labels(&boronic)?.contains(&"B(OH)2".to_owned()));
    // C1CCCCC1B(O)[O-] keeps its boronate charge and single OH visible.
    let boronate = cyclohexyl("B", 0, 0, [("O", 0), ("O", -1)]);
    assert!(!labels(&boronate)?.contains(&"B(OH)2".to_owned()));
    let phosphine = cyclohexyl("P", 0, 0, [("C", 0), ("C", 0)]);
    assert!(labels(&phosphine)?.contains(&"PMe2".to_owned()));
    // C1CCCCC1[PH+](C)C keeps its P–H and positive charge visible.
    let phosphonium = cyclohexyl("P", 1, 1, [("C", 0), ("C", 0)]);
    assert!(!labels(&phosphonium)?.contains(&"PMe2".to_owned()));
    Ok(())
}
