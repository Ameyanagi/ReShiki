use super::*;
use crate::document::{History, Point};
use crate::equipment::geometry;
use crate::equipment::solve::{Port, adjust};
use crate::equipment::{builtin, format, placed, validate};
use std::sync::Arc;

const TUBE: &str = "reshiki/compression-tube-120-1-4";
const BORE: &str = "reshiki/compression-bulkhead-union-bored-through-1-4";

fn add(doc: &mut Document, id: &str) -> u64 {
    place(doc, builtin::find(id).unwrap())
}

fn place(doc: &mut Document, d: Arc<Definition>) -> u64 {
    let id = doc.next_id();
    let mm = placed::world_per_mm(doc, placed::DEFAULT_PT_PER_MM);
    doc.graphics
        .push(placed::graphic(id, d, Point::default(), mm));
    id
}

fn port(doc: &Document, id: u64, name: &str) -> Port {
    solve::ports(doc.graphics.iter().find(|g| g.id == id).unwrap())
        .into_iter()
        .find(|p| p.at.joint == name)
        .unwrap()
}

fn bore(doc: &mut Document, tube: u64, span: &str, distance: f64) -> u64 {
    let p = port(doc, tube, span);
    let id = add(doc, BORE);
    let b = port(doc, id, "bore");
    let c = geometry::add(b.point, geometry::mul(b.dir, b.length / 2.));
    let at = geometry::add(p.point, geometry::mul(p.dir, distance * p.mm));
    doc.translate(&[id], (at[0] - c[0]) as f32, (at[1] - c[1]) as f32);
    solve::connect(doc, &[id], &p.at, &b.at).unwrap();
    id
}

fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-3, "{a} != {b}");
}

#[test]
fn a_parts_report_counts_physical_items_and_separates_ends_from_passages() {
    let mut doc = Document::default();
    let tube = add(&mut doc, TUBE);
    let first = bore(&mut doc, tube, "span", 28.);
    let second = bore(&mut doc, tube, "span", 92.);
    let cap = add(&mut doc, "reshiki/compression-cap-1-4");
    let (end, socket) = (port(&doc, tube, "b").at, port(&doc, cap, "tube").at);
    solve::connect(&mut doc, &[cap], &end, &socket).unwrap();
    let rows = parts_list(&doc, &[]).unwrap();
    assert_eq!(rows.len(), 4);
    assert_eq!(rows.iter().filter(|r| r.tube.is_some()).count(), 1);
    let t = rows
        .iter()
        .find(|r| r.id == tube)
        .unwrap()
        .tube
        .as_ref()
        .unwrap();
    near(t.cut_length_mm, 120.);
    assert_eq!(
        (t.od_mm, t.id_mm, t.material),
        (6.35, Some(4.57), TubeMaterial::StainlessSteel)
    );
    assert!(t.ends[0].connection.is_none());
    let end = t.ends[1].connection.as_ref().unwrap();
    assert_eq!(
        (end.id, end.port.as_str(), end.port_name.as_str()),
        (cap, "tube", "tube")
    );
    assert_eq!(
        t.through
            .iter()
            .map(|p| p.fitting.id)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([first, second])
    );
    assert!(
        t.through
            .iter()
            .all(|p| p.span == "span" && p.fitting.port == "bore")
    );
    let atom = doc.add_atom("C", Point::new(900., 900.));
    let filtered = parts_list(&doc, &[tube, tube, atom]).unwrap();
    assert_eq!(
        filtered,
        [rows.iter().find(|r| r.id == tube).unwrap().clone()]
    );
    let before = parts_list(&doc, &[]).unwrap();
    for g in &mut doc.graphics {
        g.origin = g.origin.offset(91., -17.);
        g.axis_x = Point::new(g.axis_x.x * 2., g.axis_x.y * 2.);
        g.axis_y = Point::new(g.axis_y.x * 2., g.axis_y.y * 2.);
    }
    doc.graphics.reverse();
    let after = parts_list(&doc, &[]).unwrap();
    for row in before {
        assert!(after.contains(&row));
    }
    // A crossing without a seat contributes neither an end nor a passage.
    let free = add(&mut doc, TUBE);
    let row = tube_row(&doc, free).unwrap().unwrap();
    assert!(row.through.is_empty() && row.ends.iter().all(|e| e.connection.is_none()));
}

#[test]
fn editable_lengths_keep_valid_endpoint_and_bore_seats_through_undo_and_reopen() {
    let mut doc = Document::default();
    let tube = add(&mut doc, TUBE);
    let first = bore(&mut doc, tube, "span", 28.);
    let second = bore(&mut doc, tube, "span", 92.);
    let cap = add(&mut doc, "reshiki/compression-cap-1-4");
    let (end, socket) = (port(&doc, tube, "b").at, port(&doc, cap, "tube").at);
    solve::connect(&mut doc, &[cap], &end, &socket).unwrap();
    let _ = crate::transaction::reconcile(&mut doc, Document::default()).unwrap();
    let before = doc.clone();
    let old_cap = port(&doc, cap, "tube").point;
    let bore_positions = [
        port(&doc, first, "bore").point,
        port(&doc, second, "bore").point,
    ];
    assert!(
        adjust::set(&mut doc, tube, "length", Value::Number(160.))
            .unwrap()
            .is_empty()
    );
    let mm = port(&doc, tube, "span").mm;
    near((port(&doc, cap, "tube").point[0] - old_cap[0]) / mm, 40.);
    assert_eq!(
        [
            port(&doc, first, "bore").point,
            port(&doc, second, "bore").point
        ],
        bore_positions
    );
    near(tube_row(&doc, tube).unwrap().unwrap().cut_length_mm, 160.);
    let _ = crate::transaction::reconcile(&mut doc, before.clone()).unwrap();
    let mut history = History::default();
    assert!(history.commit(before.clone(), &doc));
    let after = doc.clone();
    let reopened = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(
        parts_list(&reopened, &[]).unwrap(),
        parts_list(&doc, &[]).unwrap()
    );
    assert_eq!(reopened.seats, doc.seats);
    assert!(history.undo(&mut doc));
    assert_eq!(doc, before);
    assert!(history.redo(&mut doc));
    assert_eq!(doc, after);
    let released = adjust::set(&mut doc, tube, "length", Value::Number(80.)).unwrap();
    assert_eq!(released.len(), 1);
    assert_eq!(tube_row(&doc, tube).unwrap().unwrap().through.len(), 1);
    assert!(solve::partner(&doc, &socket).is_some());
    for view in ["section", "exterior"] {
        for id in [tube, first, cap] {
            assert!(
                adjust::set(&mut doc, id, "view", Value::State(view.into()))
                    .unwrap()
                    .is_empty()
            );
        }
        assert_eq!(doc.seats.len(), 2);
    }
}

#[test]
fn adjustable_straights_and_bend_keep_their_diameters_and_coherent_ports() {
    for id in ["reshiki/compression-tube-60-1-4", TUBE] {
        let d = builtin::find(id).unwrap();
        for length in [20., 87.5, 1000.] {
            let values = Values::from([("length".into(), Value::Number(length))]);
            let e = eval::evaluate(&d, &eval::resolve(&d, &values).unwrap()).unwrap();
            near(e.joints[0].at[0], 0.);
            near(e.joints[1].at[0], length);
            near(
                geometry::len(geometry::sub(e.holds[0].to, e.holds[0].from)),
                length,
            );
            near(cut_length_mm(&d, &values).unwrap().unwrap(), length);
            let wall = e.shapes.iter().find(|s| s.shape.id == "tube-wall").unwrap();
            let ys: Vec<_> = geometry::flatten(&wall.path)
                .into_iter()
                .flat_map(|(points, _)| points.into_iter().map(|p| p[1]))
                .collect();
            near(
                ys.iter().copied().fold(f64::NEG_INFINITY, f64::max)
                    - ys.iter().copied().fold(f64::INFINITY, f64::min),
                6.35,
            );
        }
    }
    let d = builtin::find("reshiki/compression-tube-bend-90-1-4").unwrap();
    for inlet in [20., 60., 125.5, 1000.] {
        for outlet in [20., 52., 103.5, 1000.] {
            let values = Values::from([
                ("inlet_length".into(), Value::Number(inlet)),
                ("outlet_length".into(), Value::Number(outlet)),
            ]);
            let e = eval::evaluate(&d, &eval::resolve(&d, &values).unwrap()).unwrap();
            assert_eq!(e.joints[0].at, [0., 0.]);
            near(e.joints[1].at[0], inlet + 20.);
            near(e.joints[1].at[1], outlet + 20.);
            near(e.holds[0].to[0], inlet);
            near(e.holds[1].from[0], inlet + 20.);
            near(e.holds[1].to[1], outlet + 20.);
            near(
                cut_length_mm(&d, &values).unwrap().unwrap(),
                inlet + outlet + 10. * std::f64::consts::PI,
            );
            assert!(e.shapes.iter().all(|s| !s.path.is_empty()));
        }
    }
}

#[test]
fn tube_spec_rejects_bad_dimensions_ends_and_length_bindings() {
    let d = builtin::find(TUBE).unwrap();
    for edit in 0..7 {
        let mut bad = (*d).clone();
        let s = bad.tube_spec.as_mut().unwrap();
        match edit {
            0 => s.od_mm = 0.,
            1 => s.id_mm = Some(s.od_mm),
            2 => s.ends[1] = s.ends[0].clone(),
            3 => s.length.params = vec!["view".into()],
            4 => s.length.params = vec!["length".into(), "length".into()],
            5 => s.length.fixed_mm = -1.,
            _ => s.od_mm = 6.,
        }
        assert!(
            validate::validate(&bad).is_err(),
            "accepted invalid tube spec {edit}"
        );
    }
}

#[test]
fn original_stock_tubes_report_without_upgrading_their_embedded_definitions() {
    for text in [
        include_str!("parts_list_legacy/compression-tube-60-1-4.reshiki-piece"),
        include_str!("parts_list_legacy/compression-tube-120-1-4.reshiki-piece"),
        include_str!("parts_list_legacy/compression-tube-bend-90-1-4.reshiki-piece"),
    ] {
        let d = Arc::new(format::parse(text).unwrap());
        let key = placed::key(&d);
        assert!(!serde_json::to_string(&d).unwrap().contains("tube_spec"));
        let mut doc = Document::default();
        let id = place(&mut doc, d.clone());
        let before = doc.clone();
        let rows = parts_list(&doc, &[]).unwrap();
        assert!(rows[0].tube.is_some());
        assert_eq!(doc, before);
        let back = Document::from_json(&doc.file_json().unwrap()).unwrap();
        assert_eq!(back.graphics[0].piece.as_ref().unwrap().definition, key);
        assert_eq!(parts_list(&back, &[]).unwrap(), rows);
        // Same names/ID do not authorize dimensions for modified geometry.
        let mut changed = (*d).clone();
        changed.shapes[0].path.push_str(" M500 0L501 0");
        let mut other = Document::default();
        let altered = place(&mut other, Arc::new(changed));
        assert!(tube_row(&other, altered).unwrap().is_none());
        assert_eq!(parts_list(&other, &[altered]).unwrap().len(), 1);
        assert_eq!(doc.graphics[0].id, id);
    }
}
