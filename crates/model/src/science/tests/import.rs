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
