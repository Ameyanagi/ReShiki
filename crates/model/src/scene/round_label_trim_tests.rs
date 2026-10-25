use super::*;

fn peroxide(joined: bool) -> Document {
    let mut doc = Document::default();
    let a = doc.add_atom("O", Point::new(-57.873432, -33.78776));
    let b = doc.add_atom("O", Point::new(-33.53651, -3.9308639));
    doc.add_bond(a, b, 1, "plain");
    if joined {
        let c = doc.add_atom("C", Point::new(-58.181698, -72.16287));
        let d = doc.add_atom("C", Point::new(3.789986, 4.592971));
        doc.add_bond(c, a, 1, "plain");
        doc.add_bond(b, d, 1, "plain");
    }
    doc
}

fn inside(p: Point, polygon: &[Point]) -> bool {
    crate::polygon::closed_edges(polygon)
        .filter(|(a, b)| {
            (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x
        })
        .count()
        % 2
        == 1
}

fn paints(p: Point, primitive: &Primitive) -> bool {
    match primitive {
        Primitive::Polygon(points) => inside(p, points),
        Primitive::Path {
            commands,
            filled: true,
            ..
        } => crate::graphics::flattened(commands)
            .iter()
            .any(|points| inside(p, points)),
        Primitive::Line(a, b, width) => {
            let length = a.distance(*b);
            let t = ((p.x - a.x) * (b.x - a.x) + (p.y - a.y) * (b.y - a.y)) / (length * length);
            (0. ..=1.).contains(&t)
                && p.distance(a.offset((b.x - a.x) * t, (b.y - a.y) * t)) < width * 0.5
        }
        Primitive::Opacity { alpha, primitive } => *alpha > 0. && paints(p, primitive),
        _ => false,
    }
}

#[test]
fn missing_round_glyph_connector_survives_scene_joins_and_rail_retrim() {
    for joined in [false, true] {
        let doc = peroxide(joined);
        let a = &doc.atoms[0];
        let b = &doc.atoms[1];
        let margin = doc.drawing_style.world(doc.drawing_style.margin_width_pt);
        let old = trimmed_span(
            a.position,
            b.position,
            &atom_label_ink_boxes(a, &doc),
            &atom_label_ink_boxes(b, &doc),
            margin,
        )
        .unwrap();
        assert!(
            old.along <= 0.1,
            "The fixture must exercise a genuinely missing connector"
        );
        let (start, end) = bond_segment(&doc, &doc.bonds[0], None)
            .expect("rounded O outlines leave a real connector");
        assert!(start.distance(end) > doc.drawing_style.line_width());
        let point = start.offset((end.x - start.x) * 0.5, (end.y - start.y) * 0.5);
        assert!(
            primitives(&doc).iter().any(|part| paints(point, part)),
            "The picked span must have an actual scene primitive: joined={joined}"
        );
        assert!(
            matches!(
                crate::arrow_anchors::pick(&doc, point, 0.25),
                Some(crate::arrow_anchors::Pick::Bond { a: 1, b: 2, .. })
            ),
            "The actual mechanism-arrow picker must reach the visible connector"
        );
    }
}

#[test]
fn rounded_dilation_keeps_margin_and_stroke_clear_of_a_round_outline() {
    let origin = Point::default();
    let unit = 0.5f32.sqrt();
    let shape = [((Point::new(-10., -10.), Point::new(10., 10.)), 10.)];
    let end = rounded_label_end(origin, unit, unit, &shape, 100., 3.);
    assert!((origin.distance(end) - 13.).abs() < 1e-4);
    let away = rounded_label_end(Point::new(30., 30.), unit, unit, &shape, 100., 3.);
    assert_eq!(away, Point::new(30., 30.));
    let capped = rounded_label_end(origin, unit, unit, &shape, 5., 3.);
    assert!((origin.distance(capped) - 5.).abs() < 1e-4);
}

#[test]
fn rectangular_unknown_and_underlined_ink_cannot_gain_a_false_span() {
    let a = Point::default();
    let b = Point::new(20., 20.);
    let boxes = [(Point::new(-11., -11.), Point::new(11., 11.))];
    let other = [(Point::new(9., 9.), Point::new(31., 31.))];
    let span = trimmed_span(a, b, &boxes, &other, 4.).unwrap();
    assert!(span.along < 0.);
    let (span, changed) =
        recover_rounded_span(span, a, b, &[(boxes[0], 0.)], &[(other[0], 0.)], 4., 0.5);
    assert!(!changed);
    assert!(span.along < 0.);
    // A rectangular underline/stem band still controls the entire label even
    // when another glyph in that label has rounded corners.
    let (span, _) = recover_rounded_span(
        span,
        a,
        b,
        &[(boxes[0], 10.), (boxes[0], 0.)],
        &[(other[0], 10.), (other[0], 0.)],
        4.,
        0.5,
    );
    assert!(span.along <= 0.1);
}

#[test]
fn truly_short_and_thick_connectors_stay_hidden_and_visible_spans_stay_exact() {
    let mut doc = peroxide(false);
    doc.atoms[1].position = doc.atoms[0].position.offset(10., 10.);
    assert!(bond_segment(&doc, &doc.bonds[0], None).is_none());
    let mut doc = peroxide(false);
    doc.drawing_style.line_width_pt = 4.;
    assert!(bond_segment(&doc, &doc.bonds[0], None).is_none());
    let mut doc = peroxide(false);
    doc.atoms[1].position = doc.atoms[0].position.offset(80., 30.);
    for margin_pt in [0., 0.001, 1.6] {
        doc.drawing_style.margin_width_pt = margin_pt;
        let a = &doc.atoms[0];
        let b = &doc.atoms[1];
        let old = trimmed_span(
            a.position,
            b.position,
            &atom_label_ink_boxes(a, &doc),
            &atom_label_ink_boxes(b, &doc),
            doc.drawing_style.world(margin_pt),
        )
        .unwrap();
        assert!(old.along > 0.1);
        assert_eq!(
            bond_segment(&doc, &doc.bonds[0], None),
            Some((old.start, old.end))
        );
    }
}

#[test]
fn a_connector_covered_by_unpadded_label_corners_is_not_exposed() {
    let a = Point::default();
    let b = Point::new(21.5, 21.5);
    let a_shapes = [((Point::new(-11., -11.), Point::new(11., 11.)), 11.)];
    let b_shapes = [((Point::new(10.5, 10.5), Point::new(32.5, 32.5)), 11.)];
    // Round-outline-only trimming would show ink through these corners, but
    // existing atom picking owns the entire unpadded rectangular backbone.
    let unit = 0.5f32.sqrt();
    let start = rounded_label_end(a, unit, unit, &a_shapes, 100., 2.4 + 0.14583333);
    let end = rounded_label_end(b, -unit, -unit, &b_shapes, 100., 2.4 + 0.14583333);
    assert!(end.x > start.x);
    let old = trimmed_span(a, b, &[a_shapes[0].0], &[b_shapes[0].0], 2.4).unwrap();
    let (kept, recovered) = recover_rounded_span(old, a, b, &a_shapes, &b_shapes, 2.4, 0.14583333);
    assert!(!recovered);
    assert!(kept.along <= 0.1);
}

/// Derive the separation from measured glyph bounds instead of assuming that a
/// platform font will hit an overlap on a fixed 0.1-unit sampling grid.
fn measured_gap_fixture(target: f32) -> (Document, TrimmedSpan, Point) {
    let mut doc = peroxide(false);
    doc.drawing_style.line_width_pt = 0.1;
    doc.drawing_style.margin_width_pt = 0.8;
    let a = doc.atoms[0].position;
    doc.atoms[1].position = a.offset(25., 25.);
    let measure = |doc: &Document| {
        trimmed_span(
            doc.atoms[0].position,
            doc.atoms[1].position,
            &atom_label_ink_boxes(&doc.atoms[0], doc),
            &atom_label_ink_boxes(&doc.atoms[1], doc),
            0.,
        )
        .unwrap()
    };
    let original = measure(&doc);
    let shift = target - original.along;
    doc.atoms[1].position = doc.atoms[1]
        .position
        .offset(original.ux * shift, original.uy * shift);
    let span = measure(&doc);
    assert!((span.along - target).abs() < 0.001);
    let point = span.start.offset(
        (span.end.x - span.start.x) * 0.5,
        (span.end.y - span.start.y) * 0.5,
    );
    (doc, span, point)
}

fn label_contains(doc: &Document, atom: usize, point: Point, padding: f32) -> bool {
    atom_label_ink_boxes(&doc.atoms[atom], doc)
        .iter()
        .any(|&(lo, hi)| {
            point.x >= lo.x - padding
                && point.x <= hi.x + padding
                && point.y >= lo.y - padding
                && point.y <= hi.y + padding
        })
}

fn assert_thin_connector_refused(doc: &Document, point: Point) {
    assert!(bond_segment(doc, &doc.bonds[0], None).is_none());
    assert!(
        !primitives(doc).iter().any(|part| paints(point, part)),
        "A refused connector must have no actual scene stroke"
    );
}

#[test]
fn thin_stroke_overlap_refusal_keeps_actual_label_pick_priority() {
    let (doc, span, point) = measured_gap_fixture(-0.25);
    assert!(span.along < 0.);
    assert!(label_contains(&doc, 0, point, 0.));
    assert!(label_contains(&doc, 1, point, 0.));
    assert_thin_connector_refused(&doc, point);
    assert!(matches!(
        crate::arrow_anchors::pick(&doc, point, 0.01),
        Some(crate::arrow_anchors::Pick::Atom(_))
    ));
}

#[test]
fn thin_stroke_positive_gap_has_no_drawn_or_pickable_bond() {
    let (doc, span, point) = measured_gap_fixture(0.05);
    // A positive but sub-threshold gap is not label overlap. Its midpoint is
    // clear of both rectangles, including the actual picker's label padding.
    assert!(span.along > 0. && span.along <= 0.1);
    assert!(!label_contains(&doc, 0, point, 0.01 * 0.35));
    assert!(!label_contains(&doc, 1, point, 0.01 * 0.35));
    assert_thin_connector_refused(&doc, point);
    assert!(crate::arrow_anchors::pick(&doc, point, 0.01).is_none());
}
