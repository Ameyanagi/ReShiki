//! Bound selection invariants and capture exact arrangement outputs before edits.
use super::*;
use crate::{
    atom_labels::{Number, number_style},
    document::{Annotation, Arrow},
    graphics::{BracketSides, Graphic, GraphicKind, GraphicStyle, LinePattern},
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
        start_anchor: None,
        end_anchor: None,
        id: doc.next_id(),
        start: Point::new(130., 130.),
        end: Point::new(260., 130.),
        kind: "forward".into(),
        control: Some(Point::new(160., 240.)),
        cubic: None,
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
    let gallery: Document = serde_json::from_str(include_str!(
        "../../../../assets/examples/shortcut-examples.rsk"
    ))
    .unwrap();
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
fn every_arrangement_preserves_explicit_geometry_and_stable_ties() {
    // Rectangles avoid host-font metrics. Their two-world-unit strokes add one
    // unit of padding on every side. The first three form one nested group:
    // A=(-1,-1)..(31,21), B=(-1,39)..(21,61), C=(49,-1)..(61,41),
    // D=(80,94)..(102,106). Horizontal gaps are 5; vertical gaps are 3.
    let mut original = Document::default();
    for (index, (x, y, width, height)) in [
        (0., 0., 10., 10.),
        (20., 10., 10., 10.),
        (10., 0., 10., 5.),
        (0., 40., 20., 20.),
        (50., 0., 10., 40.),
        (81., 95., 20., 10.),
        (200., -100., 30., 50.), // Unselected content must stay unchanged.
    ]
    .into_iter()
    .enumerate()
    {
        let mut graphic = Graphic::dragged(
            index as u64 + 1,
            GraphicKind::Rectangle,
            Point::new(x, y),
            Point::new(x + width, y + height),
            GraphicStyle {
                width_pt: crate::style::DEFAULT.points_per_world() * 2.,
                stroke: Color::Custom([20 + index as u8, 70, 130]),
                fill: Some(Color::Custom([230, 210, 180 + index as u8])),
                pattern: if index % 2 == 0 {
                    LinePattern::Dashed
                } else {
                    LinePattern::Dotted
                },
            },
            BracketSides::Both,
            false,
        );
        graphic.layer = index as i32 - 3;
        graphic.depth = [index as f32, 3., -2.];
        original.graphics.push(graphic);
    }
    original.groups = vec![
        Group {
            id: 8,
            members: vec![1, 2],
            integral: false,
        },
        Group {
            id: 9,
            members: vec![3, 2, 1],
            integral: true,
        },
    ];
    original.validate().unwrap();
    assert_eq!(
        crate::scene::selections_bounds(&original, &[vec![1, 2, 3], vec![4], vec![5], vec![6]]),
        [
            ((-1., -1.), (31., 21.)),
            ((-1., 39.), (21., 61.)),
            ((49., -1.), (61., 41.)),
            ((80., 94.), (102., 106.)),
        ]
        .map(|((x, y), (a, b))| Some((Point::new(x, y), Point::new(a, b))))
    );
    // Expected origins for A's first rectangle, B, C and D. The grouped
    // rectangles retain offsets (20,10) and (10,0) from A's first rectangle.
    let cases = [
        (
            Arrange::AlignLeft,
            [(0., 0.), (0., 40.), (0., 0.), (0., 95.)],
        ),
        (
            Arrange::AlignRight,
            [(71., 0.), (81., 40.), (91., 0.), (81., 95.)],
        ),
        (
            Arrange::AlignTop,
            [(0., 0.), (0., 0.), (50., 0.), (81., 0.)],
        ),
        (
            Arrange::AlignBottom,
            [(0., 85.), (0., 85.), (50., 65.), (81., 95.)],
        ),
        (
            Arrange::AlignHorizontal,
            [(35.5, 0.), (40.5, 40.), (45.5, 0.), (40.5, 95.)],
        ),
        (
            Arrange::AlignVertical,
            [(0., 42.5), (0., 42.5), (50., 32.5), (81., 47.5)],
        ),
        (
            Arrange::DistributeHorizontal,
            [(27., 0.), (0., 40.), (64., 0.), (81., 95.)],
        ),
        (
            Arrange::DistributeVertical,
            [(0., 0.), (0., 70.), (50., 25.), (81., 95.)],
        ),
    ];
    for reversed in [false, true] {
        let ids = if reversed {
            vec![6, 5, 2, 4, 3, 1, 5, u64::MAX]
        } else {
            vec![4, 2, 5, 6, 1, 3, 4, u64::MAX]
        };
        assert_eq!(
            groups(&original, &ids),
            if reversed {
                vec![vec![6], vec![5], vec![2, 1, 3], vec![4], vec![u64::MAX]]
            } else {
                vec![vec![4], vec![2, 1, 3], vec![5], vec![6], vec![u64::MAX]]
            }
        );
        for (action, origins) in cases {
            let origins = match (action, reversed) {
                (Arrange::DistributeHorizontal, true) => {
                    [(0., 0.), (37., 40.), (64., 0.), (81., 95.)]
                }
                (Arrange::DistributeVertical, true) => {
                    [(0., 45.), (0., 70.), (50., 0.), (81., 95.)]
                }
                _ => origins,
            }
            .map(|(x, y)| Point::new(x, y));
            let mut expected = original.clone();
            expected.graphics[0].origin = origins[0];
            expected.graphics[1].origin = origins[0].offset(20., 10.);
            expected.graphics[2].origin = origins[0].offset(10., 0.);
            for (graphic, origin) in expected.graphics[3..6].iter_mut().zip(&origins[1..]) {
                graphic.origin = *origin;
            }
            let mut actual = original.clone();
            arrange(&mut actual, &ids, action);
            assert_eq!(actual, expected, "{action:?}, reversed ties: {reversed}");
        }
    }
    for (action, _) in cases {
        for ids in [
            vec![],
            vec![u64::MAX],
            vec![2, 1, 3],
            vec![2, 1, 3, u64::MAX],
        ] {
            let mut actual = original.clone();
            arrange(&mut actual, &ids, action);
            assert_eq!(
                actual, original,
                "{action:?}: fewer than two measurable units {ids:?}"
            );
        }
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
