use super::*;
use reshiki::document::{Annotation, History, Point};

fn drawing() -> Document {
    let mut doc = Document::default();
    let a = doc.add_atom("N", Point::new(0., 0.));
    let b = doc.add_atom("O", Point::new(42., 0.));
    doc.add_bond(a, b, 1, "plain");
    doc.annotations.push(Annotation {
        id: 100,
        position: Point::new(0., 60.),
        text: "Caption H2O".into(),
        format: Default::default(),
    });
    doc
}

fn check(cache: &mut SceneCache, doc: &Document, ids: &[u64]) {
    let (markers, primitives) = cache.render(doc, ids);
    assert_eq!(
        format!("{primitives:?}"),
        format!("{:?}", scene::primitives(doc)),
    );
    assert_eq!(*markers, super::super::markers::Markers::new(doc, ids));
    for (zoom, x, width) in [(1., 0., 400.), (0.5, 150., 800.), (2., -20., 300.)] {
        let camera = Camera {
            center: Point::new(x, -15.),
            zoom,
        };
        let bounds = Rectangle::with_size(iced::Size::new(width, 300.));
        assert_eq!(
            format!("{:?}", cache.selection(doc, ids, camera, bounds)),
            format!("{:?}", SelectionBox::new(doc, ids, camera, bounds)),
        );
    }
}

#[test]
fn document_edits_loads_and_history_cannot_reuse_stale_geometry() {
    let mut cache = SceneCache::default();
    let mut doc = drawing();
    let ids = doc.all_ids();
    check(&mut cache, &doc, &ids);
    let first = cache.render(&doc, &ids).1;
    let markers = cache.render(&doc, &ids).0;
    assert!(Rc::ptr_eq(&markers, &cache.render(&doc, &ids).0));
    assert!(Rc::ptr_eq(&first, &cache.render(&doc, &ids).1));
    // The UI may mutate the same document allocation without changing IDs.
    let before = doc.clone();
    doc.atoms[0].position.x -= 25.;
    doc.bonds[0].order = 2;
    doc.annotations[0].text = "A longer edited caption".into();
    doc.annotations[0].format.style.bold = true;
    doc.annotations[0].format.style.size_pt = 18.;
    let mut history = History::default();
    assert!(history.commit(before, &doc));
    check(&mut cache, &doc, &ids);
    assert!(!Rc::ptr_eq(&first, &cache.render(&doc, &ids).1));
    assert!(!Rc::ptr_eq(&markers, &cache.render(&doc, &ids).0));
    assert!(history.undo(&mut doc));
    check(&mut cache, &doc, &ids);
    assert!(history.redo(&mut doc));
    check(&mut cache, &doc, &ids);
    // Load another drawing with reused IDs, then load an empty drawing.
    doc = drawing();
    doc.atoms[0].element = "Cl".into();
    check(&mut cache, &doc, &ids);
    check(&mut cache, &Document::default(), &[]);
    assert!(cache.render(&Document::default(), &[]).1.is_empty());
}

#[test]
fn selection_changes_do_not_invalidate_scene_or_claim_partial_moves_are_rigid() {
    let doc = drawing();
    let ids = doc.all_ids();
    let mut cache = SceneCache::default();
    let scene = cache.render(&doc, &ids).1;
    for selection in [&ids[..], &ids[..1], &ids[1..], &[][..], &ids[..]] {
        check(&mut cache, &doc, selection);
        assert_eq!(cache.whole_document(&doc, selection), selection == ids);
        assert!(Rc::ptr_eq(&scene, &cache.render(&doc, selection).1));
    }
    assert!(!cache.whole_document(&doc, &[ids[0], ids[0], 999]));
    assert!(!cache.whole_document(&Document::default(), &[]));
}

#[test]
fn guide_layouts_outlive_other_selection_reads_but_not_edits() {
    let mut doc = drawing();
    let ids = doc.all_ids();
    let mut cache = SceneCache::default();
    let dragged = &ids[..2];
    let first = cache.guides(&doc, dragged).unwrap();
    // The pointer reads the current selection's box on every event of a drag.
    let bounds = Rectangle::with_size(iced::Size::new(400., 300.));
    let _ = cache.selection(&doc, &ids[2..], Camera::default(), bounds);
    assert!(Rc::ptr_eq(&first, &cache.guides(&doc, dragged).unwrap()));
    doc.atoms[0].position.x -= 25.;
    assert!(!Rc::ptr_eq(&first, &cache.guides(&doc, dragged).unwrap()));
}

fn nmr_drawing() -> (Document, reshiki::nmr::Report) {
    let doc = Document::from_json(include_bytes!(
        "../../../tests/fixtures/nmr/ethyl-acetate.rsk"
    ))
    .unwrap();
    let request = reshiki::nmr::prepare(&doc, &[]).unwrap();
    let report = reshiki::nmr::predict(&request, reshiki::nmr::Nucleus::C13).unwrap();
    (doc, report)
}

fn nmr_context(report: &reshiki::nmr::Report) -> super::super::nmr::Context<'_> {
    super::super::nmr::Context {
        report,
        mode: super::super::nmr::LabelMode::Atom,
        supported: None,
    }
}

fn nmr_camera() -> Camera {
    Camera {
        center: Point::new(282.8, 219.),
        zoom: 2.5,
    }
}

#[test]
fn moved_atoms_invalidate_nmr_placements_without_changing_the_report() {
    let (mut doc, report) = nmr_drawing();
    let mut cache = SceneCache::default();
    let bounds = Rectangle::with_size(iced::Size::new(900., 600.));
    let context = nmr_context(&report);
    let first = cache.nmr_assignments(&doc, context, nmr_camera(), bounds);
    assert_eq!(
        first.as_ref(),
        super::super::nmr::placements(&doc, context, nmr_camera(), bounds)
    );
    assert!(Rc::ptr_eq(
        &first,
        &cache.nmr_assignments(&doc, context, nmr_camera(), bounds)
    ));
    doc.atom_mut(2).unwrap().position.x += 30.;
    let moved = cache.nmr_assignments(&doc, context, nmr_camera(), bounds);
    assert!(!Rc::ptr_eq(&first, &moved));
    assert_eq!(
        moved.as_ref(),
        super::super::nmr::placements(&doc, context, nmr_camera(), bounds)
    );
    let before = first.iter().find(|label| label.atom_id == 2).unwrap();
    let after = moved.iter().find(|label| label.atom_id == 2).unwrap();
    assert_eq!(after.anchor, before.anchor + iced::Vector::new(75., 0.));
    assert_eq!(
        reshiki::nmr::fingerprint(&doc, &report.atom_ids).unwrap(),
        report.fingerprint
    );
}

#[test]
fn nmr_placements_use_mode_camera_bits_full_bounds_and_report_values() {
    let (doc, mut report) = nmr_drawing();
    let mut cache = SceneCache::default();
    let bounds = Rectangle::with_size(iced::Size::new(900., 600.));
    let camera = nmr_camera();
    let first = cache.nmr_assignments(&doc, nmr_context(&report), camera, bounds);
    // Selection and equivalent reports do not affect assignment geometry.
    let _ = cache.render(&doc, &[2]);
    let equivalent = report.clone();
    assert!(Rc::ptr_eq(
        &first,
        &cache.nmr_assignments(&doc, nmr_context(&equivalent), camera, bounds)
    ));
    for changed in [
        Camera {
            center: camera.center.offset(1., 0.),
            ..camera
        },
        Camera {
            center: camera.center.offset(0., 1.),
            ..camera
        },
        Camera { zoom: 3., ..camera },
    ] {
        let before = cache.nmr_assignments(&doc, nmr_context(&report), camera, bounds);
        let after = cache.nmr_assignments(&doc, nmr_context(&report), changed, bounds);
        assert!(!Rc::ptr_eq(&before, &after));
        assert_eq!(
            after.as_ref(),
            super::super::nmr::placements(&doc, nmr_context(&report), changed, bounds)
        );
    }
    let zero_camera = Camera {
        center: Point::new(0., 0.),
        ..camera
    };
    for changed in [
        Camera {
            center: Point::new(-0., 0.),
            ..zero_camera
        },
        Camera {
            center: Point::new(0., -0.),
            ..zero_camera
        },
    ] {
        let before = cache.nmr_assignments(&doc, nmr_context(&report), zero_camera, bounds);
        let after = cache.nmr_assignments(&doc, nmr_context(&report), changed, bounds);
        assert!(!Rc::ptr_eq(&before, &after));
    }
    for changed in [
        Rectangle { x: -0., ..bounds },
        Rectangle { y: -0., ..bounds },
        Rectangle {
            width: 901.,
            ..bounds
        },
        Rectangle {
            height: 601.,
            ..bounds
        },
    ] {
        let before = cache.nmr_assignments(&doc, nmr_context(&report), camera, bounds);
        let after = cache.nmr_assignments(&doc, nmr_context(&report), camera, changed);
        assert!(!Rc::ptr_eq(&before, &after));
        assert_eq!(
            after.as_ref(),
            super::super::nmr::placements(&doc, nmr_context(&report), camera, changed)
        );
    }
    for mode in [
        super::super::nmr::LabelMode::Shift,
        super::super::nmr::LabelMode::Both,
    ] {
        let before = cache.nmr_assignments(&doc, nmr_context(&report), camera, bounds);
        let context = super::super::nmr::Context {
            mode,
            ..nmr_context(&report)
        };
        let after = cache.nmr_assignments(&doc, context, camera, bounds);
        assert!(!Rc::ptr_eq(&before, &after));
        assert_eq!(
            after.as_ref(),
            super::super::nmr::placements(&doc, context, camera, bounds)
        );
    }
    let before = cache.nmr_assignments(&doc, nmr_context(&report), camera, bounds);
    report.rows[0].statistics.as_mut().unwrap().median += 1.;
    let after = cache.nmr_assignments(&doc, nmr_context(&report), camera, bounds);
    assert!(!Rc::ptr_eq(&before, &after));
    assert_eq!(
        after.as_ref(),
        super::super::nmr::placements(&doc, nmr_context(&report), camera, bounds)
    );
    let before = after;
    report.rows.swap(0, 1);
    let reordered = cache.nmr_assignments(&doc, nmr_context(&report), camera, bounds);
    assert!(!Rc::ptr_eq(&before, &reordered));
    assert_eq!(
        reordered.as_ref(),
        super::super::nmr::placements(&doc, nmr_context(&report), camera, bounds)
    );
}

#[test]
fn cached_foreground_orbitals_follow_edits_and_preserve_hit_selection() {
    use reshiki::graphics::{BracketSides, Graphic, GraphicKind, GraphicStyle};
    let mut doc = drawing();
    let mut cache = SceneCache::default();
    assert!(!cache.foreground_orbital(&doc));
    let mut orbital = Graphic::dragged(
        doc.next_id(),
        GraphicKind::Orbital(reshiki::scientific::OrbitalKind::S),
        Point::default(),
        Point::new(0., -42.),
        GraphicStyle::default(),
        BracketSides::Both,
        false,
    );
    orbital.layer = 1;
    doc.graphics.push(orbital);
    for layer in [1, -1, 0] {
        doc.graphics[0].layer = layer;
        let flag = cache.foreground_orbital(&doc);
        assert_eq!(flag, layer >= 0);
        assert_eq!(cache.foreground_orbital(&doc), flag);
        for p in [Point::default(), Point::new(-30., 0.), Point::new(42., 0.)] {
            assert_eq!(
                super::super::hit::hit_selection_with_orbital(&doc, p, 1., flag),
                super::super::hit::hit_selection(&doc, p, 1.)
            );
        }
    }
    doc.graphics.clear();
    assert!(!cache.foreground_orbital(&doc));
}
