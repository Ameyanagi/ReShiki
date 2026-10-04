//! Bound selection invariants and capture exact arrangement outputs before edits.
use super::*;
use crate::{
    atom_labels::{Number, number_style},
    document::{Annotation, Arrow},
    graphics::{BracketSides, Graphic, GraphicKind, GraphicStyle},
    grouping::Group,
    palette::Color,
};

fn geometry() -> Document {
    let mut doc = Document::default();
    for (x, y) in [(0., 0.), (100., 80.), (240., -40.)] {
        let a = doc.add_atom("N", Point::new(x, y));
        let b = doc.add_atom("O", Point::new(x + 42., y));
        doc.add_bond(a, b, 1, "bold");
        doc.bonds.last_mut().unwrap().highlight = Some(Color::Custom([190, 230, 240]));
        doc.bonds.last_mut().unwrap().cip_label = Some("E".into());
        doc.bonds.last_mut().unwrap().indicator.show = Some(true);
        let atom = doc.atom_mut(a).unwrap();
        atom.charge = 1;
        atom.isotope = 15;
        atom.explicit_h = 2;
        atom.no_implicit = true;
        atom.display.number = Some(Number {
            text: format!("{a}"),
            offset: Some(Point::new(-18., 24.)),
            style: number_style(),
        });
        atom.display.highlight = Some(Color::Custom([240, 225, 190]));
    }
    doc.annotations.push(Annotation {
        id: doc.next_id(),
        position: Point::new(-20., 90.),
        text: "NH2 · grouped caption".into(),
        format: Default::default(),
    });
    doc.annotations[0].format.style.underline = true;
    doc.arrows.push(Arrow {
        id: doc.next_id(),
        start: Point::new(130., 130.),
        end: Point::new(260., 130.),
        kind: "forward".into(),
        control: Some(Point::new(160., 240.)),
        style: None,
    });
    doc.graphics.push(Graphic::dragged(
        doc.next_id(),
        GraphicKind::Ellipse,
        Point::new(310., -60.),
        Point::new(360., 80.),
        GraphicStyle::default(),
        BracketSides::Both,
        false,
    ));
    doc.validate().unwrap();
    doc
}

fn fixtures() -> Vec<(&'static str, Document)> {
    let plain = geometry();
    let mut nested = plain.clone();
    nested.group_selection(&[1, 7]).unwrap();
    nested.group_selection(&[1, 8]).unwrap();
    nested.validate().unwrap();
    let mut attachments = plain.clone();
    crate::attachments::add(
        &mut attachments,
        &[3, 4],
        crate::attachments::Kind::MultiCenter,
    )
    .unwrap();
    crate::attachments::add(
        &mut attachments,
        &[5, 6],
        crate::attachments::Kind::Variable,
    )
    .unwrap();
    attachments.validate().unwrap();
    // Crossing memberships are invalid to save, but groups/arrange are public
    // operations and have never required a validated Document.
    let mut overlap = plain.clone();
    overlap.groups = vec![
        Group {
            id: 10,
            members: vec![1, 2, 7],
            integral: false,
        },
        Group {
            id: 11,
            members: vec![2, 7, 8],
            integral: false,
        },
    ];
    let mut ties = Document::default();
    for (x, y) in [(0., 0.), (0., 0.), (0., 50.), (50., 0.)] {
        ties.add_atom("N", Point::new(x, y));
    }
    let gallery: Document =
        serde_json::from_str(include_str!("../../assets/examples/shortcut-examples.rsk")).unwrap();
    vec![
        ("geometry", plain),
        ("nested", nested),
        ("attachments", attachments),
        ("overlap", overlap),
        ("ties", ties),
        ("gallery", gallery),
    ]
}

fn selections(doc: &Document) -> [Vec<u64>; 7] {
    let all = doc.all_ids();
    let reversed: Vec<_> = all.iter().rev().copied().collect();
    let duplicated: Vec<_> = all.iter().chain(&reversed).copied().collect();
    let dangling: Vec<_> = std::iter::once(u64::MAX)
        .chain(all.iter().copied())
        .collect();
    [
        all,
        reversed,
        duplicated,
        dangling,
        vec![],
        vec![u64::MAX],
        vec![1],
    ]
}

#[test]
fn group_bounds_stay_disjoint_for_selection_edge_cases() {
    for (name, original) in fixtures() {
        for ids in selections(&original) {
            let units = groups(&original, &ids);
            let mut seen = HashSet::new();
            for id in units.iter().flatten() {
                assert!(seen.insert(*id), "{name}: overlapping output units {ids:?}");
            }
            assert_eq!(seen, ids.iter().copied().collect());
            let seeds: Vec<_> = units
                .iter()
                .map(|unit| ids.iter().position(|id| *id == unit[0]).unwrap())
                .collect();
            assert!(
                seeds.windows(2).all(|pair| pair[0] < pair[1]),
                "{name}: seed ordering changed"
            );
            for (unit, extent) in units
                .iter()
                .zip(crate::scene::selections_bounds(&original, &units))
            {
                assert_eq!(
                    extent,
                    crate::scene::selection_bounds(&original, unit),
                    "{name}: {ids:?}"
                );
            }
        }
    }
}

#[test]
fn arrangement_builds_highlight_joins_once_and_keeps_single_unit_measurement() {
    let original = geometry();
    for (ids, expected) in [
        (original.all_ids(), 1),
        (vec![1, 2], 1),
        (vec![], 0),
        (vec![u64::MAX], 0),
    ] {
        let mut doc = original.clone();
        let before = crate::bond_joins::construction_count();
        arrange(&mut doc, &ids, Arrange::AlignLeft);
        assert_eq!(
            crate::bond_joins::construction_count() - before,
            expected,
            "{ids:?}"
        );
    }
}

#[test]
#[ignore = "Baseline/candidate exact-document comparison; requires an artifact directory"]
fn arrangement_matches_captured_baseline() {
    let directory = std::path::PathBuf::from(
        std::env::var("RESHIKI_ARRANGE_OUTPUTS")
            .expect("Set RESHIKI_ARRANGE_OUTPUTS to the baseline artifact directory"),
    );
    let capture = std::env::var("RESHIKI_ARRANGE_CAPTURE_BASELINE").as_deref() == Ok("1");
    let fixtures = fixtures();
    let metadata = serde_json::to_value(&fixtures).unwrap();
    if capture {
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("fixtures.json"),
            serde_json::to_vec(&metadata).unwrap(),
        )
        .unwrap();
    } else {
        let expected: serde_json::Value =
            serde_json::from_slice(&std::fs::read(directory.join("fixtures.json")).unwrap())
                .unwrap();
        assert_eq!(
            metadata, expected,
            "Arrangement fixtures changed after baseline capture"
        );
    }
    let actions = [
        Arrange::AlignLeft,
        Arrange::AlignRight,
        Arrange::AlignTop,
        Arrange::AlignBottom,
        Arrange::AlignHorizontal,
        Arrange::AlignVertical,
        Arrange::DistributeHorizontal,
        Arrange::DistributeVertical,
    ];
    let mut cases = 0;
    for (name, original) in fixtures {
        for (selection, ids) in selections(&original).into_iter().enumerate() {
            for action in actions {
                let mut actual = original.clone();
                arrange(&mut actual, &ids, action);
                let path = directory.join(format!("{name}_{selection}_{action:?}.json"));
                if capture {
                    std::fs::write(&path, serde_json::to_vec(&actual).unwrap()).unwrap();
                } else {
                    let expected: Document =
                        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
                    assert_eq!(actual, expected, "{name}: {action:?}, {ids:?}");
                }
                cases += 1;
            }
        }
    }
    println!(
        "ARRANGE,{cases},{}",
        if capture { "captured" } else { "matched" }
    );
}
