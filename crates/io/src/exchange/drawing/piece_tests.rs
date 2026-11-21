use super::*;
use crate::chemistry::cdxml;
use crate::equipment::{builtin, placed};

#[test]
fn authored_open_outlines_and_details_keep_widths_in_cdxml_and_cdx() {
    use crate::equipment::authoring::Draft;
    use crate::graphics::{Graphic, GraphicKind, GraphicStyle};
    let mut source = Document::default();
    for (id, y, width_pt) in [(1, 0., 0.6), (2, 10., 0.4)] {
        source.graphics.push(Graphic::dragged(
            id,
            GraphicKind::Line,
            Point::new(0., y),
            Point::new(40., y),
            GraphicStyle {
                width_pt,
                ..Default::default()
            },
            Default::default(),
            false,
        ));
    }
    let mut draft = Draft::from_selection(&source, &[1, 2]).unwrap();
    draft
        .calibrate([Point::default(), Point::new(40., 0.)], 20.)
        .unwrap();
    let doc = draft.accept(&source, "lab/strokes").unwrap().document;
    let request = Request::molecule("export", doc.clone());
    let xml = write(&doc, (&request).into()).unwrap();
    let decoded = crate::exchange::from_cdx(&crate::exchange::to_cdx(&xml).unwrap()).unwrap();
    for xml in [&xml, &decoded] {
        let imported = cdxml::import_cdxml(xml).unwrap().document;
        assert_eq!(imported.graphics.len(), 2);
        assert!(imported.graphics.iter().all(|g| g.style.fill.is_none()));
        let mut widths: Vec<_> = imported.graphics.iter().map(|g| g.style.width_pt).collect();
        widths.sort_by(f32::total_cmp);
        for (actual, expected) in widths.iter().zip([0.4, 0.6]) {
            assert!((actual - expected).abs() < 1e-3, "{widths:?}");
        }
    }
}

#[test]
fn a_piece_exports_as_one_group_of_curves_in_paint_order() {
    let mut document = Document::default();
    let scale = placed::world_per_mm(&document, placed::DEFAULT_PT_PER_MM);
    let flask = builtin::find("reshiki/rbf-100-15").unwrap();
    document
        .graphics
        .push(placed::graphic(1, flask, Point::new(100., 100.), scale));
    document.validate().unwrap();
    let painted = placed::primitives(&document.graphics[0], document.canvas_theme).len();
    let request = Request::molecule("export", document.clone());
    let xml = write(&document, (&request).into()).unwrap();
    let decoded = crate::exchange::from_cdx(&crate::exchange::to_cdx(&xml).unwrap()).unwrap();
    for xml in [&xml, &decoded] {
        let imported = cdxml::import_cdxml(xml).unwrap().document;
        assert!(imported.graphics.iter().all(|g| g.piece.is_none()));
        // Compound paths may arrive as several curves; all stay in one group.
        assert!(imported.graphics.len() >= painted);
        assert!(
            imported
                .groups
                .iter()
                .any(|g| g.members.len() == imported.graphics.len())
        );
        // White glass over its doubled outline, as on the canvas.
        assert!(
            imported
                .graphics
                .iter()
                .any(|g| { g.style.fill == Some(crate::palette::Color::Custom([255; 3])) })
        );
    }
}

#[test]
fn a_clamped_flask_exports_as_one_group_with_the_fingers_around_the_neck() {
    use crate::equipment::solve;
    let mut document = Document::default();
    let scale = placed::world_per_mm(&document, placed::DEFAULT_PT_PER_MM);
    for (id, piece, at) in [
        (1, "reshiki/rbf-100-15", Point::new(100., 100.)),
        (2, "reshiki/clamp-3finger", Point::new(0., 0.)),
    ] {
        let d = builtin::find(piece).unwrap();
        document.graphics.push(placed::graphic(id, d, at, scale));
    }
    let port = |doc: &Document, piece: u64, name: &str| {
        let g = doc.graphics.iter().find(|g| g.id == piece).unwrap();
        let p = solve::ports(g)
            .into_iter()
            .find(|p| p.at.joint == name)
            .unwrap();
        [
            p.point[0] + p.dir[0] * p.length / 2.,
            p.point[1] + p.dir[1] * p.length / 2.,
        ]
    };
    let (grip, jaw) = (port(&document, 1, "neck-grip"), port(&document, 2, "jaw"));
    document.translate(&[2], (grip[0] - jaw[0]) as f32, (grip[1] - jaw[1]) as f32);
    solve::snap(&mut document, &[2], 14., false).unwrap();
    assert_eq!(document.seats.len(), 1);
    let painted: usize = (document.graphics.iter())
        .map(|g| placed::primitives(g, document.canvas_theme).len())
        .sum();
    let request = Request::molecule("export", document.clone());
    let xml = write(&document, (&request).into()).unwrap();
    let imported = cdxml::import_cdxml(&xml).unwrap().document;
    assert!(imported.graphics.len() >= painted);
    // One group holds both pieces: ChemDraw keeps a group together in the
    // stacking order, so only one group can put the fingers on both sides.
    assert!(
        imported
            .groups
            .iter()
            .any(|g| g.members.len() == imported.graphics.len()),
        "{:?}",
        imported.groups
    );
    // Clamp metal comes before the flask's white glass (the back fingers)
    // and after it (the front finger).
    let fills: Vec<_> = imported.graphics.iter().map(|g| g.style.fill).collect();
    let metal = Some(crate::palette::Color::Custom([215, 220, 224]));
    let glass = Some(crate::palette::Color::Custom([255; 3]));
    let first_glass = fills.iter().position(|f| *f == glass).unwrap();
    let last_glass = fills.iter().rposition(|f| *f == glass).unwrap();
    assert!(fills.iter().position(|f| *f == metal).unwrap() < first_glass);
    assert!(fills.iter().rposition(|f| *f == metal).unwrap() > last_glass);
}

#[test]
fn pieces_grouped_apart_then_seated_export_as_one_nested_group() {
    use crate::equipment::solve;
    use crate::graphics::{Graphic, GraphicKind, GraphicStyle};
    let mut document = Document::default();
    let scale = placed::world_per_mm(&document, placed::DEFAULT_PT_PER_MM);
    for (id, piece, at) in [
        (1, "reshiki/rbf-100-15", Point::new(100., 100.)),
        (2, "reshiki/stopper-15", Point::new(300., 0.)),
    ] {
        let d = builtin::find(piece).unwrap();
        document.graphics.push(placed::graphic(id, d, at, scale));
    }
    for (id, at) in [(3, Point::new(-100., 0.)), (4, Point::new(500., 0.))] {
        let far = Point::new(at.x + 20., at.y + 20.);
        let style = GraphicStyle::default();
        let g = Graphic::dragged(
            id,
            GraphicKind::Rectangle,
            at,
            far,
            style,
            Default::default(),
            false,
        );
        document.graphics.push(g);
    }
    for (id, members) in [(5, vec![1, 3]), (6, vec![2, 4])] {
        document.groups.push(crate::grouping::Group {
            id,
            members,
            integral: false,
            layout: None,
        });
    }
    let (s, m, _) = solve::options(&document, &[2]).remove(0);
    solve::connect(&mut document, &[2], &s.at, &m.at).unwrap();
    let request = Request::molecule("export", document.clone());
    let xml = write(&document, (&request).into()).unwrap();
    let imported = cdxml::import_cdxml(&xml).unwrap().document;
    // Both rectangles stay grouped with the assembly's curves.
    assert!(
        imported
            .groups
            .iter()
            .any(|g| g.members.len() == imported.graphics.len()),
        "{:?}",
        imported.groups
    );
}
