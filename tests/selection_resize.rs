use reshiki::{
    atom_text::{self, Mode},
    document::{Annotation, Document, Point},
    editing,
    engine::{LocalEngine, Request},
    graphics::{Graphic, GraphicKind},
    haworth::{Anomer, Sugar, sugar_document},
};

#[tokio::test]
async fn independent_scaling_preserves_sugar_identity_and_native_exports()
-> Result<(), Box<dyn std::error::Error>> {
    let engine = LocalEngine::default();
    let original = sugar_document(Sugar::Glucose, Anomer::Alpha, 42.)?;
    let expected = engine
        .request(Request::molecule("analyze", original.clone()))
        .await?
        .analysis
        .ok_or("Original properties")?;
    for (x, y) in [(0.6, 1.), (1.8, 1.), (1., 0.6), (1., 1.8)] {
        let mut doc = original.clone();
        let ids = doc.all_ids();
        editing::scale_axes_about(&mut doc, &ids, Point::default(), x, y);
        doc.validate()?;
        assert_eq!(doc.bonds, original.bonds);
        for (atom, old) in doc.atoms.iter().zip(&original.atoms) {
            assert_eq!(atom.stereo, old.stereo);
            assert_eq!(atom.depth, old.depth);
            assert!((atom.position.x - old.position.x * x).abs() < 0.001);
            assert!((atom.position.y - old.position.y * y).abs() < 0.001);
        }
        let actual = engine
            .request(Request::molecule("analyze", doc.clone()))
            .await?
            .analysis
            .ok_or("Resized properties")?;
        assert_eq!(actual.inchikey, expected.inchikey);
        assert_eq!(actual.formula, expected.formula);
        let reopened: Document = serde_json::from_slice(&serde_json::to_vec(&doc)?)?;
        assert_eq!(reopened, doc);
        for format in ["svg", "png", "pdf"] {
            assert!(!reshiki::export::drawing(&doc, format)?.is_empty());
        }
    }
    Ok(())
}

#[test]
fn groups_graphics_and_captions_follow_axis_scaling_without_stretching_fonts()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = Document::default();
    let n = doc.add_atom("N", Point::new(-42., 0.));
    let c = doc.add_atom("C", Point::new(0., 0.));
    doc.add_bond(n, c, 1, "plain");
    doc = atom_text::apply(&doc, c, "Boc", Mode::Auto)?;
    let group = doc.abbreviation(c).ok_or("Group")?.members.clone();
    let hidden = doc
        .atoms
        .iter()
        .find(|a| group.contains(&a.id) && a.id != c)
        .ok_or("Hidden atom")?
        .clone();
    let graphic = Graphic::dragged(
        doc.next_id(),
        GraphicKind::Rectangle,
        Point::new(90., 10.),
        Point::new(130., 30.),
        Default::default(),
        Default::default(),
        false,
    );
    let graphic_id = graphic.id;
    doc.graphics.push(graphic.clone());
    let caption = Annotation {
        id: doc.next_id(),
        position: Point::new(120., 70.),
        text: "Example".into(),
        format: Default::default(),
    };
    let caption_id = caption.id;
    doc.annotations.push(caption.clone());
    let original = doc.clone();
    editing::scale_axes_about(
        &mut doc,
        &[c, graphic_id, caption_id],
        Point::default(),
        1.5,
        0.5,
    );
    assert_eq!(doc.atom(n), original.atom(n));
    assert_eq!(
        doc.atom(hidden.id).ok_or("Hidden atom")?.position,
        Point::new(hidden.position.x * 1.5, hidden.position.y * 0.5)
    );
    assert_eq!(doc.abbreviations, original.abbreviations);
    let moved = doc.graphics.first().ok_or("Graphic")?;
    assert_eq!(
        moved.origin,
        Point::new(graphic.origin.x * 1.5, graphic.origin.y * 0.5)
    );
    assert_eq!(
        moved.axis_x,
        Point::new(graphic.axis_x.x * 1.5, graphic.axis_x.y * 0.5)
    );
    assert_eq!(
        moved.axis_y,
        Point::new(graphic.axis_y.x * 1.5, graphic.axis_y.y * 0.5)
    );
    assert_eq!(moved.style, graphic.style);
    let moved = doc.annotations.first().ok_or("Caption")?;
    assert_eq!(moved.position, Point::new(180., 35.));
    assert_eq!(moved.format, caption.format);
    doc.validate()?;
    Ok(())
}

#[test]
fn resized_ring_interiors_and_centroids_follow_the_ring() -> Result<(), Box<dyn std::error::Error>>
{
    // Geometric bounds exclude stroke width, which intentionally stays fixed.
    let ellipse_bounds = |g: Graphic| {
        let center = g.origin.offset(
            (g.axis_x.x + g.axis_y.x) / 2.,
            (g.axis_x.y + g.axis_y.y) / 2.,
        );
        let rx = g.axis_x.x.hypot(g.axis_y.x) / 2.;
        let ry = g.axis_x.y.hypot(g.axis_y.y) / 2.;
        (center.offset(-rx, -ry), center.offset(rx, ry))
    };
    for tilt in [0., 45.] {
        let mut source = Document::default();
        let ids = editing::ring(&mut source, Point::new(80., 60.), 6, true, 0.);
        reshiki::projection::tilt(&mut source, &ids, tilt, true);
        let centroid = reshiki::projection::add_centroid(&mut source, &ids)?;
        let circle = reshiki::aromatic::circles(&source)
            .into_iter()
            .next()
            .ok_or("Original circle")?
            .graphic();
        let (old_lo, old_hi) = ellipse_bounds(circle);
        for (x, y) in [(1.8, 1.), (1., 0.6)] {
            let mut doc = source.clone();
            editing::scale_axes_about(&mut doc, &ids, Point::default(), x, y);
            let (lo, hi) = ellipse_bounds(
                reshiki::aromatic::circles(&doc)
                    .into_iter()
                    .next()
                    .ok_or("Resized ellipse")?
                    .graphic(),
            );
            assert!(
                (lo.x - old_lo.x * x).abs() < 0.1 && (hi.x - old_hi.x * x).abs() < 0.1,
                "tilt {tilt}, scale {x}/{y}: {old_lo:?}..{old_hi:?} -> {lo:?}..{hi:?}"
            );
            assert!((lo.y - old_lo.y * y).abs() < 0.1 && (hi.y - old_hi.y * y).abs() < 0.1);
            let old = source.atom(centroid).ok_or("Original centroid")?;
            let point = doc.atom(centroid).ok_or("Moved centroid")?;
            assert!(
                point
                    .position
                    .distance(Point::new(old.position.x * x, old.position.y * y))
                    < 0.001
            );
            assert_eq!(doc.bonds, source.bonds);
        }
    }
    Ok(())
}
