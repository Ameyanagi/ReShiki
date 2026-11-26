//! Results read from other programs' files, attached to the opened molecule.
use super::*;

#[test]
fn import_attaches_exact_frame_to_the_selected_models_sources() {
    let (mut doc, [first, second]) = water_pair();
    let mut frame = (*launch(&doc, second, 0, 1).unwrap().frame).clone();
    frame.positions_angstrom[0][0] = 0.123_456_789_123;
    frame.drawing_atoms = vec![Some(9001), None, None];
    let calculation = imported(result(&frame, Task::Energy, true));
    let models = doc.models.clone();
    let attached = import(&mut doc, 1, frame.clone(), vec![], calculation.clone()).unwrap();
    assert_eq!(attached.freshness, Freshness::Current);
    let attachment = doc.science.attachment(attached.attachment).unwrap();
    assert_eq!(attachment.atoms, [Some(second), None, None]);
    assert_eq!(attachment.frame, frame.hash());
    assert_eq!(doc.science.frame(&frame.hash()).unwrap().as_ref(), &frame);
    assert_eq!(doc.science.results[0].result.as_ref(), &calculation);
    assert!(results_for(&doc, first).is_empty());
    assert_eq!(results_for(&doc, second)[0].freshness, Freshness::Current);
    assert_eq!(doc.models, models);
    assert_eq!(launch(&doc, second, 0, 1).unwrap().frame.as_ref(), &frame);
    doc.science.validate().unwrap();
}

#[test]
fn import_optimization_attaches_its_exact_input_and_final_frames() {
    let (mut doc, oxygen) = water();
    let frame = (*launch(&doc, oxygen, 0, 1).unwrap().frame).clone();
    let mut input = frame.clone();
    input.positions_angstrom[0][0] += 0.123_456_789_123;
    let mut calculation = imported(result(&frame, Task::Optimize, false));
    calculation.input_frame = Some(input.hash());
    let attached = import(&mut doc, 0, frame.clone(), vec![input.clone()], calculation).unwrap();
    assert_eq!(attached.freshness, Freshness::Current);
    assert_eq!(
        doc.science.attachment(attached.attachment).unwrap().frame,
        input.hash()
    );
    assert_eq!(doc.science.frame(&input.hash()).unwrap().as_ref(), &input);
    assert_eq!(doc.science.frame(&frame.hash()).unwrap().as_ref(), &frame);
    assert_eq!(results_for(&doc, oxygen)[0].frame.as_ref(), &frame);
    doc.science.validate().unwrap();
}

#[test]
fn import_uses_the_current_chemistry_as_its_baseline() {
    let (mut doc, oxygen) = water();
    let mut frame = (*launch(&doc, oxygen, 0, 1).unwrap().frame).clone();
    frame.charge = 1;
    frame.multiplicity = 2;
    let atom = doc.atom_mut(oxygen).unwrap();
    atom.charge = 1;
    atom.radical_electrons = 1;
    assert!(doc.models[0].is_stale(&doc));
    // Imported geometry may differ from the display's f32 conversion.
    frame.positions_angstrom[1][0] += 0.000_9;
    let calculation = imported(result(&frame, Task::Energy, false));
    let attached = import(&mut doc, 0, frame, vec![], calculation).unwrap();
    assert_eq!(attached.freshness, Freshness::Current);
    let attachment = doc.science.attachment(attached.attachment).unwrap();
    assert_eq!(attachment.baseline, baseline(&doc, &[oxygen]));
    assert_eq!(attachment.freshness(&doc), Freshness::Current);
    doc.atom_mut(oxygen).unwrap().charge = 0;
    assert_eq!(
        doc.science
            .attachment(attached.attachment)
            .unwrap()
            .freshness(&doc),
        Freshness::Stale
    );
}

#[test]
fn import_rejects_element_and_position_mismatches_without_changing_the_document() {
    let (mut doc, oxygen) = water();
    let frame = (*launch(&doc, oxygen, 0, 1).unwrap().frame).clone();
    let mut position = frame.clone();
    // Each axis is within tolerance, but the position is more than 1e-3 Å away.
    for axis in &mut position.positions_angstrom[0] {
        *axis += 0.000_6;
    }
    let mut hydrogens = frame.clone();
    hydrogens.positions_angstrom.swap(1, 2);
    let mut element = frame;
    element.numbers[0] = 16;
    let before = doc.clone();
    for frame in [position, hydrogens, element] {
        let calculation = imported(result(&frame, Task::Energy, false));
        assert!(matches!(
            import(&mut doc, 0, frame, vec![], calculation),
            Err(ScienceError::ModelMismatch)
        ));
        assert_eq!(doc, before);
    }
}

#[test]
fn import_keeps_the_result_of_a_complex_of_several_molecules() {
    // One file, one model, two waters: a dimer's result is for both.
    let mut doc = Document::default();
    let oxygens = [
        doc.add_atom("O", Point::new(0., 0.)),
        doc.add_atom("O", Point::new(60., 0.)),
    ];
    let atoms = vec![
        Placed {
            id: oxygens[0],
            xyz: [0., 0., 0.],
        },
        Placed {
            id: oxygens[1],
            xyz: [2.9, 0., 0.],
        },
    ];
    let hydrogen = |parent, xyz| Hydrogen { parent, xyz };
    let hydrogens = vec![
        hydrogen(oxygens[0], [0.95, 0.2, 0.]),
        hydrogen(oxygens[0], [-0.3, 0.9, 0.]),
        hydrogen(oxygens[1], [3.2, 0.9, 0.]),
        hydrogen(oxygens[1], [3.2, -0.9, 0.]),
    ];
    doc.models
        .push(Model3d::new(&doc, atoms, hydrogens, ForceField::Mmff94s));
    crate::models3d::require_version(&mut doc);
    let model = &doc.models[0];
    let frame = Frame {
        numbers: vec![8, 8, 1, 1, 1, 1],
        positions_angstrom: model
            .atoms
            .iter()
            .map(|a| a.xyz)
            .chain(model.hydrogens.iter().map(|h| h.xyz))
            .map(|p| p.map(f64::from))
            .collect(),
        isotopes: vec![],
        masses_amu: vec![],
        charge: 0,
        multiplicity: 1,
        drawing_atoms: vec![],
    };
    let calculation = imported(result(&frame, Task::Energy, true));
    let attached = import(&mut doc, 0, frame, vec![], calculation).unwrap();
    assert_eq!(attached.freshness, Freshness::Current);
    for oxygen in oxygens {
        assert_eq!(results_for(&doc, oxygen).len(), 1);
    }
    // A third molecule bonded to one of them makes the complex another one.
    let carbon = doc.add_atom("C", Point::new(0., 40.));
    doc.add_bond(oxygens[0], carbon, 1, "plain");
    let link = doc.science.attachment(attached.attachment).unwrap();
    assert_eq!(link.freshness(&doc), Freshness::Stale);
}
