use super::*;
use crate::{
    cycles::{Cycle, Shape, Step},
    document::Annotation,
    graphics::{Graphic, GraphicKind, GraphicStyle},
    typography::TextFormat,
};

fn text(doc: &mut Document, center: Point, label: &str) -> u64 {
    let id = doc.next_id();
    let mut annotation = Annotation {
        id,
        angle: 0.,
        position: Point::default(),
        text: label.into(),
        format: TextFormat {
            style: doc.drawing_style.text_style(),
            ..Default::default()
        },
    };
    let (w, h) = annotation.size();
    annotation.position = center.offset(-w * 0.5, -h * 0.5);
    doc.annotations.push(annotation);
    id
}

fn graphic(doc: &Document, id: u64) -> &Graphic {
    doc.graphics.iter().find(|g| g.id == id).unwrap()
}

fn side_ids(doc: &Document, id: u64, step: usize) -> Vec<Vec<u64>> {
    let step = &graphic(doc, id).cycle.as_ref().unwrap().steps[step];
    step.reagents
        .iter()
        .chain(&step.products)
        .cloned()
        .collect()
}

fn side_routes(doc: &Document, id: u64, step: usize) -> Vec<Arrow> {
    let mut drawn = cycles::Drawn::default();
    let mut owners = Owners::default();
    let g = graphic(doc, id);
    resolve_owned(
        doc,
        g,
        &cycles::outline(g).unwrap(),
        &mut drawn,
        &mut owners,
        |_, _| {},
    );
    drawn
        .arrows
        .into_iter()
        .zip(drawn.arcs)
        .zip(owners.arrows)
        .filter_map(|((arrow, arc), owner)| (arc.is_none() && owner == Some(step)).then_some(arrow))
        .collect()
}

// Check final public rendering independently of the repair's Scene plan:
// every side route ends on a painted native lane, avoids its molecule/caption,
// and does not cross any foreign step's painted curve.
fn assert_clear_contacts(doc: &Document, id: u64, step: usize) {
    let g = graphic(doc, id);
    let layout = Layout::graphic(doc, g).unwrap();
    let painted = &layout.steps[step].as_ref().unwrap().arrows;
    let routes = side_routes(doc, id, step);
    let ids = side_ids(doc, id, step);
    assert_eq!(routes.len(), ids.len());
    for (route, ids) in routes.iter().zip(ids) {
        let incoming = route.appearance().head == crate::arrows::Head::None;
        let touch = if incoming { route.end } else { route.start };
        let (shaft, projection) = painted
            .iter()
            .filter_map(|a| {
                let shaft = Shaft::from_arrow(a)?;
                let p = shaft.project(touch)?;
                Some((shaft, p))
            })
            .min_by(|(_, a), (_, b)| a.distance.total_cmp(&b.distance))
            .unwrap();
        assert!(
            projection.distance < 0.01,
            "side route misses painted lane: {projection:?}"
        );
        let tangent = shaft.sample(projection.s, !incoming).unwrap().1;
        let controls = route.cubic.unwrap();
        let way = if incoming {
            Point::new(route.end.x - controls[1].x, route.end.y - controls[1].y)
        } else {
            Point::new(controls[0].x - route.start.x, controls[0].y - route.start.y)
        };
        assert!(
            way.x * tangent.x + way.y * tangent.y > 0.,
            "contact goes against its painted lane"
        );
        let ink = super::super::part_ink(doc, &ids);
        assert!(
            ink.iter().all(|ink| !arrow_hits(route, *ink, 0.)),
            "connector enters its molecule/caption"
        );
        for (other, resolved) in layout.steps.iter().enumerate().filter(|(s, _)| *s != step) {
            for arrow in &resolved.as_ref().unwrap().arrows {
                let a: Vec<_> = (0..=512).map(|k| route.point(k as f32 / 512.)).collect();
                let b: Vec<_> = (0..=512).map(|k| arrow.point(k as f32 / 512.)).collect();
                assert!(
                    !crossing(&a, &b, 0.05),
                    "side connector crosses foreign step {other}"
                );
                assert!(
                    ink.iter().all(|ink| !arrow_hits(arrow, *ink, 0.05)),
                    "foreign step enters inherited molecule"
                );
            }
        }
    }
}

fn qa(before: &Document, after: &Document) {
    let Some(dir) = std::env::var_os("RESHIKI_CYCLE_FIT_SIDE_QA_DIR") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (name, doc) in [("before", before), ("after", after)] {
        std::fs::write(dir.join(format!("{name}.rsk")), doc.file_json().unwrap()).unwrap();
        // One fixed world camera gives directly comparable before/after PNGs.
        let svg = crate::scene::svg_part(
            &crate::scene::primitives(doc),
            Point::new(-1000., -700.),
            Point::new(1000., 1500.),
        );
        assert!(!svg.contains("NaN"));
        std::fs::write(dir.join(format!("{name}.svg")), &svg).unwrap();
        let mut options = resvg::usvg::Options::default();
        options.fontdb_mut().load_system_fonts();
        let tree = resvg::usvg::Tree::from_str(&svg, &options).unwrap();
        let mut image = resvg::tiny_skia::Pixmap::new(1000, 1100).unwrap();
        image.fill(resvg::tiny_skia::Color::WHITE);
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(0.5, 0.5),
            &mut image.as_mut(),
        );
        image.save_png(dir.join(format!("{name}.png"))).unwrap();
    }
}

#[test]
fn actual_suzuki_bow180_split_repairs_stranded_sides_on_native_child_lanes() {
    let before = Document::from_json(include_bytes!(
        "../../../../tests/fixtures/cycles/suzuki-rectangle-bow180-target.rsk"
    ))
    .unwrap();
    let part: Vec<_> = (137..=143).collect();
    let target = cycles::drop_target(&before, &part).unwrap();
    assert_eq!(target.role, Role::Intermediate(2));
    let id = target.cycle;
    let old = graphic(&before, id).cycle.as_ref().unwrap();
    assert_eq!(old.shape, Shape::Rectangle);
    assert_eq!((old.steps[1].bow_pt, old.steps[1].reversible), (180., true));
    let proposal = cycles::propose_drop(&before, &part)
        .expect("the captured insertion must have a complete clear plan");
    let after = proposal.doc;
    let new = graphic(&after, id).cycle.as_ref().unwrap();
    assert_eq!(new.intermediates[2], part);
    assert_eq!(
        new.steps[1], old.steps[1],
        "inherited chemistry, name and settings remain exact"
    );
    assert_eq!(new.steps[2], Step::default());
    assert_eq!(
        (after.atoms.len(), after.bonds.len()),
        (before.atoms.len(), before.bonds.len())
    );
    assert_eq!(after.bonds, before.bonds);
    let inherited = side_ids(&before, id, 1);
    assert!(
        inherited
            .iter()
            .any(|ids| crate::scene::selection_bounds(&before, ids)
                != crate::scene::selection_bounds(&after, ids)),
        "the regression must exercise a moved inherited side part"
    );
    for ids in inherited {
        let a = center_of(crate::scene::selection_bounds(&before, &ids).unwrap());
        let b = center_of(crate::scene::selection_bounds(&after, &ids).unwrap());
        assert_eq!(
            cycles::outline(graphic(&before, id)).unwrap().contains(a),
            cycles::outline(graphic(&after, id)).unwrap().contains(b)
        );
        let delta = Point::new(b.x - a.x, b.y - a.y);
        for atom in before.atoms.iter().filter(|a| ids.contains(&a.id)) {
            let moved = after.atoms.iter().find(|a| a.id == atom.id).unwrap();
            let mut expected = atom.clone();
            expected.position = expected.position.offset(delta.x, delta.y);
            assert!(expected.position.distance(moved.position) < 0.002);
            expected.position = moved.position;
            assert_eq!(&expected, moved, "only the atom position may change");
        }
    }
    assert_clear_contacts(&after, id, 1);
    assert_eq!(
        Document::from_json(&after.file_json().unwrap()).unwrap(),
        after
    );
    assert_eq!(
        cycles::propose_drop(&before, &part).unwrap().doc,
        after,
        "bounded search must be deterministic"
    );
    qa(&before, &after);
}

fn synthetic(
    side_fraction: f64,
    cut_fraction: f64,
    mirrored: bool,
    ccw: bool,
) -> (Document, Document, Target, u64) {
    let mut before = Document::default();
    let mut g = Graphic::dragged(
        1,
        GraphicKind::Ellipse,
        Point::new(-1200., -700.),
        Point::new(1200., 700.),
        GraphicStyle::default(),
        Default::default(),
        false,
    );
    if mirrored {
        g.origin.x += g.axis_x.x;
        g.axis_x.x = -g.axis_x.x;
    }
    g.cycle = Some(Box::new(Cycle {
        shape: Shape::Rectangle,
        counterclockwise: ccw,
        ..Default::default()
    }));
    before.graphics.push(g);
    let a = text(&mut before, Point::new(0., -700.), "Pd");
    let b = text(&mut before, Point::new(0., 700.), "Pd");
    let c = before.graphics[0].cycle.as_mut().unwrap();
    c.intermediates = vec![vec![a], vec![b]];
    c.steps = vec![
        Step {
            bow_pt: 180.,
            reversible: true,
            name: "retained donor".into(),
            ..Default::default()
        },
        Step::default(),
    ];
    let g = &before.graphics[0];
    let layout = Layout::graphic(&before, g).unwrap();
    let arrows = &layout.steps[0].as_ref().unwrap().arrows;
    let shafts: Vec<_> = arrows.iter().map(Shaft::from_arrow).collect();
    let frame = Frame::at(
        [shafts[0].as_ref(), shafts[1].as_ref()],
        &layout.outline,
        0.,
        side_fraction,
        if ccw { -1. } else { 1. },
    )
    .unwrap();
    let gap = DEFAULT.world(g.cycle.as_ref().unwrap().gap_pt);
    let bond = before.drawing_style.bond_length_world;
    let side_center =
        cycles::tidy_shape::place(frame, &layout.outline, (60., 50.), gap + bond, bond, false)
            .unwrap();
    let cut = shafts[0]
        .as_ref()
        .unwrap()
        .sample(shafts[0].as_ref().unwrap().length() * cut_fraction, false)
        .unwrap()
        .0;
    let station = layout.insertion(0, 0, cut).unwrap();
    let side = text(&mut before, side_center, "OH");
    before.graphics[0].cycle.as_mut().unwrap().steps[0]
        .reagents
        .push(vec![side]);
    let free = text(&mut before, cut, "X");
    before.validate().unwrap();
    let target = Target {
        cycle: 1,
        role: Role::Intermediate(1),
        part: vec![free],
        ink: crate::scene::selection_bounds(&before, &[free]).unwrap(),
        arc: vec![],
        tag: "test source cut".into(),
    };
    let mut after = before.clone();
    assert!(
        after.graphics[0]
            .cycle
            .as_mut()
            .unwrap()
            .insert_intermediate(1, vec![free], Some(station))
    );
    (before, after, target, side)
}

#[test]
fn late_inherited_side_is_repaired_with_reflection_and_reverse_direction() {
    for mirrored in [false, true] {
        for ccw in [false, true] {
            let (before, mut after, target, side) = synthetic(0.75, 0.35, mirrored, ccw);
            let old = before
                .annotations
                .iter()
                .find(|a| a.id == side)
                .unwrap()
                .clone();
            assert!(repair(&before, &mut after, &target), "{mirrored}/{ccw}");
            let new = after.annotations.iter().find(|a| a.id == side).unwrap();
            assert_ne!(
                new.position, old.position,
                "a side beyond the child must move"
            );
            let mut expected = old;
            expected.position = new.position;
            assert_eq!(&expected, new);
            assert_clear_contacts(&after, 1, 0);
            after.validate().unwrap();
        }
    }
}

#[test]
fn early_clear_side_is_left_exactly_in_place_after_a_late_cut() {
    let (before, mut after, target, _) = synthetic(0.2, 0.85, false, false);
    let source = after.clone();
    assert!(repair(&before, &mut after, &target));
    assert_eq!(
        after, source,
        "unaffected inherited side ink must not be tidied"
    );
    assert_clear_contacts(&after, 1, 0);
}

#[test]
fn inherited_molecule_moves_rigidly_with_real_name_and_property_captions() {
    // This shortened child retains a vertical section. A molecule with
    // captions below it can then join from the side without crossing its text.
    let (mut before, raw, target, old_side) = synthetic(0.85, 0.65, false, false);
    let run = &raw.graphics[0].cycle.as_ref().unwrap().split_runs[0];
    let station = crate::cycles::split_runs::SplitStation {
        u: run.cuts[0],
        start: run.source_start,
        span: run.source_span,
    };
    let at = center_of(crate::scene::selection_bounds(&before, &[old_side]).unwrap());
    before.annotations.retain(|a| a.id != old_side);
    let a = before.add_atom("C", at.offset(-21., 0.));
    let b = before.add_atom("C", at.offset(21., 0.));
    before.add_bond(a, b, 1, "plain");
    let atoms = vec![a, b];
    before.graphics[0].cycle.as_mut().unwrap().steps[0].reagents = vec![atoms.clone()];
    let identity = crate::molecule_names::identity(&before, &atoms).unwrap();
    crate::molecule_names::show(
        &mut before,
        &atoms,
        "ethane".into(),
        identity,
        Default::default(),
        false,
    )
    .unwrap();
    crate::linked_properties::toggle(
        &mut before,
        &atoms,
        crate::linked_properties::Property::Weight,
        Default::default(),
        false,
    )
    .unwrap();
    before.validate().unwrap();
    let linked = crate::molecule_names::include_annotations(&before, &atoms);
    assert!(linked.len() > atoms.len());
    let old_layout = Layout::graphic(&before, &before.graphics[0]).unwrap();
    let old_station = old_layout
        .station(
            0,
            0,
            center_of(crate::scene::selection_bounds(&before, &atoms).unwrap()),
        )
        .unwrap();
    assert!(
        old_station > station.u,
        "the caption-preservation fixture must still be stranded beyond the cut"
    );
    let mut after = before.clone();
    assert!(
        after.graphics[0]
            .cycle
            .as_mut()
            .unwrap()
            .insert_intermediate(1, target.part.clone(), Some(station))
    );
    assert!(repair(&before, &mut after, &target));
    assert!(
        before.property_captions[0]
            .properties
            .contains(&crate::linked_properties::Property::Weight)
    );
    let first = before.atom(a).unwrap().position;
    let next = after.atom(a).unwrap().position;
    let delta = Point::new(next.x - first.x, next.y - first.y);
    assert!(delta.distance(Point::default()) > 1.);
    assert!(
        after
            .atom(b)
            .unwrap()
            .position
            .distance(before.atom(b).unwrap().position.offset(delta.x, delta.y))
            < 0.003
    );
    assert_eq!(after.bonds, before.bonds);
    assert_eq!(after.molecule_names, before.molecule_names);
    assert_eq!(after.property_captions, before.property_captions);
    for caption in before.annotations.iter().filter(|t| linked.contains(&t.id)) {
        let moved = after
            .annotations
            .iter()
            .find(|t| t.id == caption.id)
            .unwrap();
        assert!(
            moved
                .position
                .distance(caption.position.offset(delta.x, delta.y))
                < 0.003
        );
        let mut expected = caption.clone();
        expected.position = moved.position;
        assert_eq!(&expected, moved);
    }
    assert_clear_contacts(&after, 1, 0);
    after.validate().unwrap();
}

#[test]
fn an_unpaintable_child_declines_atomically() {
    let (before, mut after, target, _) = synthetic(0.75, 0.35, false, false);
    after.graphics[0].cycle.as_mut().unwrap().steps[0].lead_pt = f32::MAX;
    let snapshot = after.clone();
    assert!(!repair(&before, &mut after, &target));
    assert_eq!(after, snapshot);
}

#[test]
fn blocked_local_positions_decline_without_partial_side_moves() {
    let (mut before, mut after, target, _) = synthetic(0.75, 0.35, false, false);
    let panel = Graphic::dragged(
        before.next_id(),
        GraphicKind::Rectangle,
        Point::new(-4000., -2500.),
        Point::new(4000., 2500.),
        GraphicStyle::default(),
        Default::default(),
        false,
    );
    before.graphics.push(panel.clone());
    after.graphics.push(panel);
    before.validate().unwrap();
    after.validate().unwrap();
    let snapshot = after.clone();
    assert!(!repair(&before, &mut after, &target));
    assert_eq!(after, snapshot);
}

#[test]
fn current_scene_validation_uses_normalized_positions_and_current_loose_ink() {
    let (before, mut after, target, _) = synthetic(0.75, 0.35, false, false);
    assert!(repair(&before, &mut after, &target));
    assert!(valid(&after, 1, 0));
    let ids = after.all_ids();
    after.translate(&ids, -135.5, 83.25);
    let normalized = after.clone();
    assert!(
        valid(&after, 1, 0),
        "a translated prepared candidate needs no old-source inference"
    );
    assert_eq!(after, normalized, "validation must not relocate anything");

    let contact = side_routes(&after, 1, 0).remove(0);
    text(&mut after, contact.point(0.5), "blocked");
    let blocked = after.clone();
    assert!(
        !valid(&after, 1, 0),
        "current loose ink must be checked after normalization"
    );
    assert_eq!(after, blocked);
}

#[test]
fn current_scene_validation_requires_fitted_metadata_and_accepts_empty_sides() {
    let (before, after, _, _) = synthetic(0.75, 0.35, false, false);
    assert!(
        !valid(&before, 1, 0),
        "an ordinary step has no fitted metadata"
    );
    assert!(!valid(&after, 1, usize::MAX));
    assert!(!valid(&after, u64::MAX, 0));
    assert!(
        valid(&after, 1, 1),
        "the new fitted child has no side participants"
    );
}

fn distant_caption_member() -> (Document, Document, Vec<u64>, u64) {
    let (mut plain, _, target, side) = synthetic(0.2, 0.5, false, false);
    let old_free = target.part[0];
    let at = center_of(crate::scene::selection_bounds(&plain, &[old_free]).unwrap());
    plain
        .annotations
        .retain(|a| a.id != side && a.id != old_free);
    plain.graphics[0].cycle.as_mut().unwrap().steps[0]
        .reagents
        .clear();
    let a = plain.add_atom("C", at.offset(-21., 0.));
    let b = plain.add_atom("C", at.offset(21., 0.));
    plain.add_bond(a, b, 1, "plain");
    let atoms = vec![a, b];
    let mut named = plain.clone();
    let identity = crate::molecule_names::identity(&named, &atoms).unwrap();
    let caption = crate::molecule_names::show(
        &mut named,
        &atoms,
        "ethane".into(),
        identity,
        Default::default(),
        false,
    )
    .unwrap();
    let from = named
        .annotations
        .iter()
        .find(|a| a.id == caption)
        .unwrap()
        .position;
    named.translate(&[caption], -2000. - from.x, -2500. - from.y);
    plain.validate().unwrap();
    named.validate().unwrap();
    (plain, named, atoms, caption)
}

fn fitted_primary(doc: &Document) -> Vec<Arrow> {
    let drawn = cycles::drawn(doc, &doc.graphics[0]);
    drawn
        .arrows
        .into_iter()
        .zip(drawn.arcs)
        .filter_map(|(arrow, step)| step.filter(|step| *step <= 1).map(|_| arrow))
        .collect()
}

fn assign_stored_member(before: &Document, atoms: &[u64]) -> Document {
    let target = cycles::drop_target(before, atoms).expect("stored member must hit its source");
    assert_eq!(target.role, Role::Intermediate(1));
    let mut after = before.clone();
    cycles::assign(&mut after, &target);
    assert!(
        atoms
            .iter()
            .all(|id| cycles::role_of(&after, *id) == Some((target.cycle, target.role))),
        "stored-geometry assignment must produce a complete fitted candidate"
    );
    assert!(super::super::valid(&after, target.cycle));
    assert!(super::super::accepted(&after, &target));
    let g = graphic(&after, target.cycle);
    let gap = DEFAULT.world(g.cycle.as_ref().unwrap().gap_pt);
    let boxes = super::super::part_ink(&after, atoms);
    assert!(
        cycles::drawn(&after, g)
            .arrows
            .iter()
            .all(|arrow| boxes.iter().all(|ink| !arrow_hits(arrow, *ink, gap - 0.01))),
        "every real body/caption box must clear complete painted shafts and heads"
    );
    after.validate().unwrap();
    after
}

#[test]
fn distant_linked_caption_does_not_enlarge_the_inserted_molecules_primary_opening() {
    let (plain, named, atoms, caption) = distant_caption_member();
    let caption_ink = crate::scene::selection_bounds(&named, &[caption]).unwrap();
    let union = crate::scene::selection_bounds(
        &named,
        &crate::molecule_names::include_annotations(&named, &atoms),
    )
    .unwrap();
    let old = cycles::drawn(&named, &named.graphics[0]);
    assert!(
        old.arrows.iter().all(|a| !arrow_hits(a, caption_ink, 0.)),
        "the separate caption must already be clear of actual curves"
    );
    let shaft = Shaft::from_arrow(&old.arrows[0]).unwrap();
    let phantom = shaft.sample(shaft.length() * 0.2, false).unwrap().0;
    let body = crate::scene::selection_bounds(&named, &atoms).unwrap();
    assert!(cycles::contains(union, phantom));
    assert!(
        !cycles::contains(body, phantom) && !cycles::contains(caption_ink, phantom),
        "the union must cover a separate source region containing neither body nor caption"
    );
    // Stored native geometry may carry a distant linked caption. Assignment
    // isolates clipping from name reconciliation's normal caption positioning.
    let without_caption = assign_stored_member(&plain, &atoms);
    let with_caption = assign_stored_member(&named, &atoms);
    assert_eq!(
        fitted_primary(&with_caption),
        fitted_primary(&without_caption),
        "primary clipping depends on molecule body, not empty space up to its caption"
    );
    assert_eq!(with_caption.molecule_names, named.molecule_names);
    assert_eq!(
        with_caption.annotations.iter().find(|a| a.id == caption),
        named.annotations.iter().find(|a| a.id == caption)
    );
    assert_eq!(with_caption.atoms, named.atoms);
    assert_eq!(with_caption.bonds, named.bonds);
    with_caption.validate().unwrap();
}

#[test]
fn distant_caption_union_cannot_retain_an_inserted_molecule_dragged_off_its_source() {
    let (plain, named, atoms, caption) = distant_caption_member();
    let applied = assign_stored_member(&named, &atoms);
    let original = cycles::drawn(&plain, &plain.graphics[0]);
    let shaft = Shaft::from_arrow(&original.arrows[0]).unwrap();
    let probe = shaft.sample(shaft.length() * 0.2, false).unwrap().0;
    let linked = crate::molecule_names::include_annotations(&applied, &atoms);
    let mut moved = applied.clone();
    let old_union = crate::scene::selection_bounds(&applied, &linked).unwrap();
    let old_caption = crate::scene::selection_bounds(&applied, &[caption]).unwrap();
    let delta = Point::new(
        probe.x - old_union.0.x - (old_caption.1.x - old_caption.0.x + 100.),
        probe.y - old_union.0.y - (old_caption.1.y - old_caption.0.y + 100.),
    );
    moved.translate(&linked, delta.x, delta.y);
    let body = crate::scene::selection_bounds(&moved, &atoms).unwrap();
    let union = crate::scene::selection_bounds(&moved, &linked).unwrap();
    let caption_ink = crate::scene::selection_bounds(&moved, &[caption]).unwrap();
    assert!(!cycles::contains(body, probe) && !cycles::contains(caption_ink, probe));
    assert!(
        cycles::contains(union, probe),
        "this regression must leave a phantom union crossing the old source"
    );
    let from = center_of(body);
    assert!(
        original
            .arrows
            .iter()
            .filter_map(Shaft::from_arrow)
            .all(|s| s.project(from).unwrap().distance
                > f64::from(4. * moved.drawing_style.bond_length_world)),
        "the actual molecule must be far from every source lane"
    );
    cycles::fit_moved(&applied, &mut moved, &linked);
    assert!(
        moved
            .take_out(&linked, delta)
            .contains(&crate::grouping::Container::Cycle),
        "only actual molecule ink may retain a fitted cut"
    );
    assert!(
        atoms
            .iter()
            .all(|id| cycles::role_of(&moved, *id).is_none())
    );
    let c = moved.graphics[0].cycle.as_ref().unwrap();
    assert!(c.split_runs.is_empty());
    assert_eq!(
        c.intermediates,
        plain.graphics[0].cycle.as_ref().unwrap().intermediates
    );
    assert_eq!(c.steps, plain.graphics[0].cycle.as_ref().unwrap().steps);
    assert_eq!(moved.molecule_names, applied.molecule_names);
    let before_caption = applied
        .annotations
        .iter()
        .find(|a| a.id == caption)
        .unwrap();
    let after_caption = moved.annotations.iter().find(|a| a.id == caption).unwrap();
    assert_eq!(
        after_caption.position,
        before_caption.position.offset(delta.x, delta.y)
    );
    assert_eq!(
        moved.atom(atoms[0]).unwrap().position,
        applied
            .atom(atoms[0])
            .unwrap()
            .position
            .offset(delta.x, delta.y)
    );
    moved.validate().unwrap();
}
