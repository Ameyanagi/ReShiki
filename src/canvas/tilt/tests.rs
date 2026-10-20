use super::*;

#[test]
fn screen_angles_snap_and_keep_turning_without_nonfinite_edits() {
    let drag = TiltDrag {
        ids: vec![],
        start: Point::new(100., 100.),
    };
    assert_eq!(drag.angles(Point::new(136., 74.), false), (13., 18.));
    assert_eq!(drag.angles(Point::new(136., 74.), true), (15., 15.));
    // A long drag keeps turning at 0.5°/px instead of stopping at 75°.
    assert_eq!(drag.angles(Point::new(500., 100.), false), (0., 200.));
    assert_eq!(drag.angles(Point::new(507., 100.), true), (0., 210.));
    assert_eq!(drag.angles(Point::new(1000., -1000.), true), (555., 450.));
    assert_eq!(drag.angles(Point::new(101., 101.), false), (0., 0.));
    assert_eq!(drag.angles(Point::new(f32::NAN, 100.), false), (0., 0.));
    let mut doc = reshiki::rings::Preset::Regular.document(42., false);
    let before = doc.clone();
    let ids = doc.all_ids();
    for (x, y) in [(f32::NAN, 15.), (15., f32::INFINITY), (720., -360.)] {
        apply(&mut doc, &ids, x, y);
        assert_eq!(doc, before);
    }
}

#[test]
fn readout_wraps_to_plus_minus_180() {
    for (raw, shown) in [
        (0., 0.),
        (179., 179.),
        (180., 180.),
        (200., -160.),
        (-200., 160.),
        (360., 0.),
        (555., -165.),
        (-450., -90.),
    ] {
        assert_eq!(wrap(raw), shown, "{raw}");
    }
    assert!(
        wrap(-0.).is_sign_positive(),
        "a horizontal drag reads X +0°"
    );
}

#[test]
fn tilts_past_75_degrees_turn_and_a_full_turn_restores_positions() {
    let original = reshiki::rings::Preset::Regular.document(42., false);
    let ids = original.all_ids();
    let close = |a: &Document, b: &Document| {
        a.atoms.iter().zip(&b.atoms).all(|(a, b)| {
            (a.position.x - b.position.x).abs() < 1e-3
                && (a.position.y - b.position.y).abs() < 1e-3
                && (a.depth - b.depth).abs() < 1e-3
        })
    };
    let mut wide = original.clone();
    apply(&mut wide, &ids, 0., 200.);
    assert!(!close(&wide, &original));
    let mut wrapped = original.clone();
    apply(&mut wrapped, &ids, 0., wrap(200.));
    assert!(close(&wide, &wrapped), "200° and −160° are one orientation");
    // A 180° Y tilt is no longer dropped: it mirrors x about the centre.
    let mut half = original.clone();
    apply(&mut half, &ids, 0., 180.);
    let n = original.atoms.len() as f32;
    let centre = original.atoms.iter().map(|a| a.position.x).sum::<f32>() / n;
    for (a, b) in half.atoms.iter().zip(&original.atoms) {
        assert!((a.position.x - (2. * centre - b.position.x)).abs() < 1e-3);
        assert!((a.position.y - b.position.y).abs() < 1e-3);
    }
    // Three 120° tilts on each axis compose to full turns.
    let mut turned = original.clone();
    for _ in 0..3 {
        apply(&mut turned, &ids, 120., 0.);
    }
    for _ in 0..3 {
        apply(&mut turned, &ids, 0., -120.);
    }
    assert!(!turned.atoms.is_empty() && close(&turned, &original));
}

#[test]
fn a_molecule_shown_in_3d_or_both_turns_its_model_and_keeps_its_drawing() {
    use reshiki::editing::{Transform, transform};
    use reshiki::models3d::Display;
    let atoms = |doc: &Document, ids: [u64; 2]| {
        ids.map(|id| {
            let a = doc.atom(id).unwrap();
            (a.position.x, a.position.y, a.depth)
        })
    };
    for display in [Display::TwoD, Display::Both, Display::ThreeD] {
        let (doc, molecule, drawn) = shown(display);
        let modelled = display != Display::TwoD;
        let mixed = [molecule, drawn].concat();
        assert!(available(&doc, &molecule) && available(&doc, &mixed));
        let tilts: [fn(&mut Document, &[u64]); 3] = [
            |doc, ids| apply(doc, ids, 30., 20.),
            |doc, ids| transform(doc, ids, Transform::TiltX(15.)),
            |doc, ids| transform(doc, ids, Transform::TiltY(-15.)),
        ];
        for tilt in tilts {
            for ids in [&molecule[..], &mixed] {
                let mut tilted = doc.clone();
                tilt(&mut tilted, ids);
                assert_ne!(tilted, doc, "{display:?}");
                assert_eq!(
                    atoms(&tilted, molecule) == atoms(&doc, molecule),
                    modelled,
                    "{display:?}"
                );
                assert_eq!(
                    tilted.models[0].rotation != doc.models[0].rotation,
                    modelled,
                    "{display:?}"
                );
            }
            // The drawn pair turns about its own centre, as if tilted alone.
            let mut both = doc.clone();
            tilt(&mut both, &mixed);
            let mut alone = doc.clone();
            tilt(&mut alone, &drawn);
            assert_ne!(atoms(&both, drawn), atoms(&doc, drawn));
            if modelled {
                assert_eq!(atoms(&both, drawn), atoms(&alone, drawn));
            }
        }
    }
}
