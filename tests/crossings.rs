use reshiki::{
    crossings,
    document::{Document, Point},
    scene::{self, Primitive},
};
fn crossed() -> Document {
    let mut doc = Document::default();
    for p in [
        Point::new(-30., 0.),
        Point::new(30., 0.),
        Point::new(0., -30.),
        Point::new(0., 30.),
    ] {
        doc.add_atom("C", p);
    }
    doc.add_bond(1, 2, 1, "plain");
    doc.add_bond(3, 4, 1, "plain");
    doc
}
#[test]
fn crossing_trims_lower_geometry_and_front_back_reverse_it() {
    let mut doc = crossed();
    let gaps = crossings::gaps(&doc);
    assert_eq!(gaps[0].len(), 1);
    assert!(gaps[1].is_empty());
    let primitives = scene::primitives(&doc);
    let lines: Vec<_> = primitives
        .iter()
        .filter_map(|p| match p {
            Primitive::Line(a, b, _) => Some((a, b)),
            _ => None,
        })
        .collect();
    assert_eq!(lines.len(), 3);
    assert!(
        lines
            .iter()
            .filter(|(a, b)| a.y == 0. && b.y == 0.)
            .all(|(a, b)| a.x * b.x > 0.)
    );
    doc.bonds[0].z_order = 1;
    let reversed = crossings::gaps(&doc);
    assert!(reversed[0].is_empty());
    assert_eq!(reversed[1].len(), 1);
    assert!(!scene::svg(&doc).contains("rgb(255,255,255)"));
    doc.bonds[0].display = "wedge".into();
    doc.bonds[0].z_order = -1;
    let outlines: Vec<_> = scene::primitives(&doc)
        .iter()
        .flat_map(|p| match p {
            Primitive::Path {
                commands,
                filled: true,
                ..
            } => reshiki::graphics::flattened(commands),
            _ => Vec::new(),
        })
        .collect();
    assert_eq!(outlines.len(), 2);
    assert!(outlines.iter().all(|p| p.first() == p.last()));
    assert!(outlines.iter().flatten().all(|p| p.x.abs() > 3.));
    let serialized = serde_json::to_string(&doc).unwrap();
    let back: Document = serde_json::from_str(&serialized).unwrap();
    assert_eq!(back.bonds[0].z_order, -1);
}
#[test]
fn connected_or_parallel_bonds_never_get_crossing_gaps() {
    let mut doc = crossed();
    doc.bonds[1].a = 1;
    assert!(crossings::gaps(&doc).iter().all(Vec::is_empty));
    doc = crossed();
    doc.atoms[2].position = Point::new(-30., 5.);
    doc.atoms[3].position = Point::new(30., 5.);
    assert!(crossings::gaps(&doc).iter().all(Vec::is_empty));
}

#[test]
fn clipping_a_closed_hollow_outline_preserves_its_endpoint_edges() {
    use reshiki::graphics::{GraphicStyle, PathCommand, flattened};
    let doc = crossed();
    let gaps = crossings::gaps(&doc);
    let corners = [
        Point::new(-30., -3.),
        Point::new(30., -3.),
        Point::new(30., 3.),
        Point::new(-30., 3.),
    ];
    let outline = Primitive::Path {
        commands: vec![
            PathCommand::Move(corners[0]),
            PathCommand::Line(corners[1]),
            PathCommand::Line(corners[2]),
            PathCommand::Line(corners[3]),
            PathCommand::Close,
        ],
        style: GraphicStyle::default(),
        filled: false,
    };
    let clipped = crossings::cut(vec![outline], &gaps[0]);
    let paths: Vec<_> = clipped
        .iter()
        .flat_map(|p| match p {
            Primitive::Path { commands, .. } => flattened(commands),
            _ => Vec::new(),
        })
        .collect();
    for [a, b] in [[corners[1], corners[2]], [corners[3], corners[0]]] {
        assert!(
            paths
                .iter()
                .any(|p| p.windows(2).any(|edge| edge == [a, b])),
            "crossing removed an endpoint edge: {a:?} to {b:?}"
        );
    }
    assert!(paths.iter().flatten().all(|p| p.x.abs() > 3.));
}

#[test]
fn crossing_gaps_split_waves_without_flattening_or_changing_their_style() {
    use reshiki::graphics::{PathCommand, flattened};
    for reverse in [false, true] {
        let mut doc = crossed();
        doc.bonds[0].display = "wavy".into();
        doc.bonds[0].color = [32, 80, 145];
        if reverse {
            doc.bonds[0].reverse();
        }
        let primitives = scene::primitives(&doc);
        let paths: Vec<_> = primitives
            .iter()
            .filter_map(|p| {
                if let Primitive::Path {
                    commands, style, ..
                } = p
                {
                    Some((commands, style))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(paths.len(), 2);
        for (commands, style) in paths {
            assert_eq!(style.stroke, [32, 80, 145]);
            assert!(commands.iter().any(|c| matches!(c, PathCommand::Cubic(..))));
            assert!(flattened(commands).iter().flatten().all(|p| p.x.abs() > 3.));
        }
        doc.bonds[0].z_order = 1;
        assert_eq!(
            scene::primitives(&doc)
                .iter()
                .filter(|p| matches!(p, Primitive::Path { .. }))
                .count(),
            1
        );
    }
}
