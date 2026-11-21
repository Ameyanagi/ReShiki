use super::*;

#[test]
fn indexed_markers_preserve_document_and_selection_order_and_abbreviation_visibility() {
    let doc: Document = serde_json::from_str(include_str!(
        "../../../assets/examples/shortcut-examples.rsk"
    ))
    .unwrap();
    let all = doc.all_ids();
    for ids in [
        all.clone(),
        all.iter().step_by(3).copied().collect(),
        vec![],
        vec![all[0], all[0], u64::MAX],
    ] {
        let markers = Markers::new(&doc, &ids);
        let bonds = doc
            .bonds
            .iter()
            .filter(|bond| doc.bond_visible(bond.a, bond.b))
            .filter(|bond| ids.contains(&bond.a) && ids.contains(&bond.b))
            .filter_map(|bond| doc.atom(bond.a).zip(doc.atom(bond.b)))
            .map(|(a, b)| (a.position, b.position))
            .collect::<Vec<_>>();
        let atoms = ids
            .iter()
            .filter_map(|id| doc.atom(*id).filter(|atom| doc.atom_visible(atom.id)))
            .map(|atom| atom.position)
            .collect::<Vec<_>>();
        let captions = doc
            .annotations
            .iter()
            .filter(|caption| ids.contains(&caption.id))
            .map(|caption| CaptionOutline::Box(caption.corners()))
            .collect::<Vec<_>>();
        assert_eq!(markers.bonds, bonds);
        assert_eq!(
            markers
                .atoms
                .iter()
                .map(|(_, atom)| *atom)
                .collect::<Vec<_>>(),
            atoms
        );
        assert_eq!(markers.captions, captions);
    }
}

#[test]
fn two_sided_group_bond_markers_meet_the_visible_label() {
    let mut doc = Document::default();
    let atoms: Vec<_> = (0..4)
        .map(|i| doc.add_atom("C", reshiki::document::Point::new(i as f32 * 42., 0.)))
        .collect();
    for pair in atoms.windows(2) {
        doc.add_bond(pair[0], pair[1], 1, "plain");
    }
    doc.contract(&atoms[1..3], "Link", "").unwrap();
    let markers = Markers::new(&doc, &doc.all_ids());
    assert_eq!(
        markers.bonds,
        vec![
            (World::new(0., 0.), World::new(42., 0.)),
            (World::new(42., 0.), World::new(126., 0.)),
        ]
    );
    assert!(!markers.atoms.iter().any(|(id, _)| *id == atoms[2]));
}

fn cell_caption() -> Document {
    let mut doc = Document::default();
    let mut table = reshiki::graphics::table::editor::dragged(
        100,
        World::new(30., 20.),
        World::new(630., 420.),
        [2, 3],
        1.2,
    )
    .unwrap();
    let frame = table.table.as_ref().unwrap().cells[0]
        .frame(&table)
        .unwrap();
    table.table.as_mut().unwrap().cells[0].members.push(1);
    doc.annotations.push(reshiki::document::Annotation {
        id: 1,
        position: frame.point(World::new(0.05, 0.08)),
        text: "Reaction conditions and solvent".into(),
        format: Default::default(),
        angle: 0.,
    });
    doc.graphics.push(table);
    doc.validate().unwrap();
    doc
}

fn polygon_bounds(points: &[World]) -> (World, World) {
    let first = points[0];
    points.iter().fold((first, first), |(lo, hi), p| {
        (
            World::new(lo.x.min(p.x), lo.y.min(p.y)),
            World::new(hi.x.max(p.x), hi.y.max(p.y)),
        )
    })
}

#[test]
fn wrapped_cell_caption_markers_and_cached_handles_follow_the_painted_layout() {
    use crate::canvas::{Handle, SelectionBox, cache::SceneCache, selection::FRAME_PADDING_PX};
    use reshiki::scene::{self, Primitive};
    let doc = cell_caption();
    let before = doc.file_json().unwrap();
    let a = &doc.annotations[0];
    let g = &doc.graphics[0];
    let frame = g.table.as_ref().unwrap().cells[0].frame(g).unwrap();
    let layout = scene::table_annotation_layout(a, frame);
    assert!(layout.height > a.size().1 * 2.);
    assert!(layout.width < a.size().0);
    let cells = scene::table::Layout::selected_annotations(&doc, |id| id == a.id).unwrap();
    let polygon = cells.annotation_polygon(a).unwrap();
    let expected = polygon_bounds(&polygon);
    assert!(expected.0.distance(a.position) < 0.001);
    assert!(
        expected
            .1
            .distance(a.position.offset(layout.width, layout.height))
            < 0.001
    );
    assert_eq!(scene::selection_bounds(&doc, &[a.id]), Some(expected));
    let markers = Markers::new(&doc, &[a.id]);
    assert_eq!(markers.captions, [CaptionOutline::Cell(polygon)]);

    let camera = Camera::default();
    let bounds = Rectangle::with_size(Size::new(1000., 800.));
    let selection = SelectionBox::new(&doc, &[a.id], camera, bounds).unwrap();
    let right = camera.screen(
        World::new(expected.1.x, (expected.0.y + expected.1.y) / 2.),
        bounds,
    ) + iced::Vector::new(FRAME_PADDING_PX, 0.);
    assert_eq!(selection.hit(right), Some(Handle::Edge(1)));
    let raw = a.bounds();
    let old_right = camera.screen(World::new(raw.1.x, (raw.0.y + raw.1.y) / 2.), bounds)
        + iced::Vector::new(FRAME_PADDING_PX, 0.);
    assert_eq!(selection.hit(old_right), None);
    assert_eq!(
        selection.pivot,
        reshiki::editing::rotation_center(&doc, &[a.id]).unwrap()
    );
    let mut cache = SceneCache::default();
    let (cached_markers, drawing) = cache.render(&doc, &[a.id]);
    assert_eq!(*cached_markers, markers);
    assert_eq!(
        cache
            .selection(&doc, &[a.id], camera, bounds)
            .unwrap()
            .drawn_edge_centers(),
        selection.drawn_edge_centers()
    );
    let Primitive::Clip { primitives, .. } = &drawing[0] else {
        panic!("Cell scene")
    };
    let rows: Vec<_> = primitives
        .iter()
        .filter_map(|p| match p {
            Primitive::Text { position, .. } => Some(position.y),
            _ => None,
        })
        .collect();
    assert!(rows.iter().any(|y| *y > a.position.y + a.size().1));
    assert_eq!(doc.file_json().unwrap(), before);
}

#[test]
fn turned_cell_caption_outlines_clip_the_same_glyph_layout_without_changing_ordinary_text() {
    use reshiki::scene::{self, Primitive};
    let mut doc = cell_caption();
    doc.graphics[0].axis_y = World::new(100., 400.);
    let frame = doc.graphics[0].table.as_ref().unwrap().cells[0]
        .frame(&doc.graphics[0])
        .unwrap();
    doc.annotations[0].position = frame.point(World::new(0.05, 0.05));
    doc.annotations[0].angle = 25.;
    let a = &doc.annotations[0];
    let source = a.clone();
    let layout = scene::table_annotation_layout(a, frame);
    let pivot = a.position.offset(layout.width / 2., layout.height / 2.);
    let corners = [
        a.position,
        a.position.offset(layout.width, 0.),
        a.position.offset(layout.width, layout.height),
        a.position.offset(0., layout.height),
    ]
    .map(|p| p.turned(pivot, a.angle));
    assert!(corners.iter().any(|p| !frame.contains(*p)));
    let cells = scene::table::Layout::new(&doc);
    let expected = cells.polygon(a.id, &corners);
    assert!(
        expected.len() >= 3
            && expected.iter().all(|p| {
                let p = frame.local(*p).unwrap();
                (-0.0001..=1.0001).contains(&p.x) && (-0.0001..=1.0001).contains(&p.y)
            })
    );
    assert_eq!(cells.annotation_polygon(a), Some(expected.clone()));
    assert_eq!(
        Markers::new(&doc, &[a.id]).captions,
        [CaptionOutline::Cell(expected.clone())]
    );
    assert_eq!(
        scene::selection_bounds(&doc, &[a.id]),
        Some(polygon_bounds(&expected))
    );
    let drawing = scene::primitives(&doc);
    let Primitive::Clip { primitives, .. } = &drawing[0] else {
        panic!("Cell scene")
    };
    let (painted_pivot, painted_angle, runs) = primitives
        .iter()
        .find_map(|p| match p {
            Primitive::Rotated {
                pivot,
                angle,
                primitives,
            } => Some((*pivot, *angle, primitives)),
            _ => None,
        })
        .unwrap();
    assert_eq!((painted_pivot, painted_angle), (pivot, a.angle));
    let mut visible = 0;
    let mut partially_clipped = false;
    let extent = polygon_bounds(&expected);
    for run in runs {
        if let Primitive::Text {
            position,
            text,
            size,
            style,
            ..
        } = run
        {
            for (lo, hi) in reshiki::style::text_ink_boxes(text, *size, style) {
                let quad = [lo, World::new(hi.x, lo.y), hi, World::new(lo.x, hi.y)]
                    .map(|p| position.offset(p.x, p.y).turned(pivot, a.angle));
                let clipped = cells.polygon(a.id, &quad);
                if !clipped.is_empty() {
                    visible += 1;
                    partially_clipped |= quad.iter().any(|p| !frame.contains(*p));
                    assert!(clipped.iter().all(|p| p.x >= extent.0.x - 0.001
                        && p.x <= extent.1.x + 0.001
                        && p.y >= extent.0.y - 0.001
                        && p.y <= extent.1.y + 0.001));
                }
            }
        }
    }
    assert!(visible > 0 && partially_clipped);
    assert_eq!(doc.annotations[0], source);

    let mut hidden = doc.clone();
    hidden.annotations[0].position = World::new(5000., -3000.);
    let before = hidden.file_json().unwrap();
    assert!(Markers::new(&hidden, &[1]).captions.is_empty());
    assert_eq!(scene::selection_bounds(&hidden, &[1]), None);
    assert_eq!(hidden.file_json().unwrap(), before);

    // Releasing ownership restores the exact independent caption box and size.
    doc.graphics[0].table.as_mut().unwrap().cells[0]
        .members
        .clear();
    assert!(scene::table::Layout::selected_annotations(&doc, |_| true).is_none());
    assert_eq!(
        Markers::new(&doc, &[1]).captions,
        [CaptionOutline::Box(source.corners())]
    );
    assert_eq!(scene::selection_bounds(&doc, &[1]), Some(source.bounds()));
    assert_eq!(doc.annotations[0], source);
}
