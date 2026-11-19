//! Placed pieces in a drawing: saving, opening, copying and the edit guards.
use super::builtin;
use super::placed::{self, PieceRef};
use crate::document::{DRAWING_ELEMENTS, Document, Point};
use crate::graphics::GraphicKind;

fn with_flask() -> Document {
    let mut doc = Document::default();
    let flask = builtin::find("reshiki/rbf-100-15").unwrap();
    let id = doc.next_id();
    let scale = placed::world_per_mm(&doc, placed::DEFAULT_PT_PER_MM);
    doc.graphics
        .push(placed::graphic(id, flask, Point::new(200., 150.), scale));
    doc
}

#[test]
fn a_placed_piece_saves_at_version_40_with_its_definition_and_reopens_the_same() {
    let doc = with_flask();
    doc.validate().unwrap();
    let bytes = doc.file_json().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["version"], DRAWING_ELEMENTS);
    assert_eq!(json["graphics"][0]["kind"], "piece");
    assert_eq!(json["piece_definitions"][0]["id"], "reshiki/rbf-100-15");
    let key = json["graphics"][0]["piece"]["definition"].as_str().unwrap();
    assert!(key.starts_with("reshiki/rbf-100-15@1#"), "{key}");
    let back = Document::from_json(&bytes).unwrap();
    assert_eq!(back.graphics, doc.graphics);
    assert!(back.graphics[0].piece.as_ref().unwrap().linked.is_some());
    assert_eq!(
        format!("{:?}", crate::scene::primitives(&back)),
        format!("{:?}", crate::scene::primitives(&doc))
    );
}

#[test]
fn a_drawing_without_the_definition_it_uses_does_not_open() {
    let doc = with_flask();
    let mut json: serde_json::Value = serde_json::from_slice(&doc.file_json().unwrap()).unwrap();
    json["piece_definitions"] = serde_json::json!([]);
    let error = Document::from_json(json.to_string().as_bytes()).unwrap_err();
    assert!(
        error.starts_with("The drawing uses the equipment piece reshiki/rbf-100-15@1#")
            && error.ends_with("but does not include it."),
        "{error}"
    );
    // An embedded definition is checked like a piece file.
    let mut json: serde_json::Value = serde_json::from_slice(&doc.file_json().unwrap()).unwrap();
    json["piece_definitions"][0]["joints"][0]["size"] = serde_json::json!("16/30");
    let error = Document::from_json(json.to_string().as_bytes()).unwrap_err();
    assert!(error.contains("not a standard taper size"), "{error}");
}

#[test]
fn the_flask_paints_glass_hatching_and_liquid_and_is_hit_inside_its_bulb() {
    let doc = with_flask();
    let g = &doc.graphics[0];
    let primitives = crate::scene::primitives(&doc);
    assert!(primitives.len() > 10, "{}", primitives.len());
    let (lo, hi) = g.shape_bounds();
    // A 100 mL flask is about 58 mm across: 23 pt at 0.4 pt per mm.
    let width_pt = (hi.x - lo.x) * doc.drawing_style.points_per_world();
    assert!((22.0..27.0).contains(&width_pt), "{width_pt}");
    // Centred on the point it was placed at.
    let mid = Point::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.);
    assert!(mid.distance(Point::new(200., 150.)) < 0.01, "{mid:?}");
    // The bulb is low in the piece; a click inside it selects the flask.
    let bulb = Point::new(mid.x, hi.y - (hi.x - lo.x) / 2.);
    assert!(g.hit(bulb, 0.1));
    assert!(!g.hit(Point::new(hi.x + 5., hi.y + 5.), 0.1));
}

#[test]
fn copying_carries_the_definitions_and_deleting_the_last_piece_drops_them() {
    let doc = with_flask();
    let part = crate::editing::selection(&doc, &[doc.graphics[0].id]);
    assert_eq!(part.piece_definitions.len(), 1);
    let clipboard = serde_json::to_string(&part.current()).unwrap();
    let pasted = Document::from_json(clipboard.as_bytes()).unwrap();
    let mut target = Document::default();
    crate::editing::append(&mut target, &pasted, Point::new(10., 0.));
    assert_eq!(target.graphics.len(), 1);
    assert_eq!(target.current().piece_definitions.len(), 1);
    let mut emptied = doc.clone();
    emptied.delete(&[doc.graphics[0].id]);
    assert!(emptied.current().piece_definitions.is_empty());
}

#[test]
fn a_piece_turns_and_scales_but_never_stretches_or_takes_point_edits() {
    let mut doc = with_flask();
    let id = doc.graphics[0].id;
    let before = doc.graphics[0].clone();
    crate::editing::transform_about(&mut doc, &[id], Point::new(0., 0.), 1.5, 30.);
    doc.validate().unwrap();
    let g = &doc.graphics[0];
    assert!(
        (g.axis_x.distance(Point::default()) / before.axis_x.distance(Point::default()) - 1.5)
            .abs()
            < 1e-4
    );
    // Stretching moves the piece with its centre and keeps its frame.
    let frame = (g.axis_x, g.axis_y);
    crate::editing::scale_axes_about(&mut doc, &[id], Point::new(0., 0.), 2., 1.);
    doc.validate().unwrap();
    assert_eq!((doc.graphics[0].axis_x, doc.graphics[0].axis_y), frame);
    let mut g = doc.graphics[0].clone();
    assert!(g.edit_point(0, Point::new(1., 1.)).is_err());
    assert_eq!(g.kind, GraphicKind::Piece);
    // Beyond 5 pt per mm a resize only moves the piece.
    let frame = (doc.graphics[0].axis_x, doc.graphics[0].axis_y);
    crate::editing::transform_about(&mut doc, &[id], Point::new(0., 0.), 20., 0.);
    doc.validate().unwrap();
    assert_eq!((doc.graphics[0].axis_x, doc.graphics[0].axis_y), frame);
}

#[test]
fn a_skewed_or_unlinked_piece_is_refused() {
    let doc = with_flask();
    let mut g = doc.graphics[0].clone();
    g.axis_y.x += 0.5;
    assert!(
        g.validate()
            .unwrap_err()
            .contains("not stretched or skewed")
    );
    let mut g = doc.graphics[0].clone();
    g.piece = Some(PieceRef {
        linked: None,
        ..g.piece.clone().unwrap()
    });
    assert!(g.validate().unwrap_err().contains("is not in the drawing"));
    let mut g = doc.graphics[0].clone();
    g.piece
        .as_mut()
        .unwrap()
        .params
        .insert("fill".into(), super::eval::Value::State("full".into()));
    assert!(g.validate().is_err());
}

#[test]
fn every_built_in_piece_places_and_draws() {
    for d in builtin::catalogue() {
        let mut doc = Document::default();
        let scale = placed::world_per_mm(&doc, placed::DEFAULT_PT_PER_MM);
        doc.graphics
            .push(placed::graphic(1, d.clone(), Point::new(0., 0.), scale));
        doc.validate().unwrap_or_else(|e| panic!("{}: {e}", d.id));
        assert!(!crate::scene::primitives(&doc).is_empty(), "{}", d.id);
    }
}
