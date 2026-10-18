use super::*;
use crate::{
    arrows::{ArrowStyle, Preset},
    document::Document,
    typography::TextFormat,
};

const ARROW: u64 = 1;

fn scheme() -> Document {
    let mut doc = Document::default();
    doc.arrows.push(Arrow::new(
        ARROW,
        Point::new(0., 0.),
        Point::new(120., 0.),
        Preset::Forward,
        ArrowStyle::preset(Preset::Forward),
    ));
    doc
}

/// Commit an edit as the editor does.
fn edit(doc: &mut Document, change: impl FnOnce(&mut Document)) {
    let before = doc.clone();
    change(doc);
    let _ = crate::transaction::reconcile(doc, before).unwrap();
}

/// Type text whose box is centred on `at`, as the text tool does.
fn type_text(doc: &mut Document, text: &str, at: Point) -> u64 {
    let id = doc.next_id();
    edit(doc, |doc| {
        let mut caption = Annotation {
            id,
            position: Point::default(),
            text: text.into(),
            format: TextFormat::default(),
        };
        let (w, h) = caption.size();
        caption.position = at.offset(-w / 2., -h / 2.);
        doc.annotations.push(caption);
    });
    id
}

fn text(doc: &Document, id: u64) -> &Annotation {
    doc.annotations.iter().find(|a| a.id == id).unwrap()
}
fn arrow(doc: &Document) -> &Arrow {
    doc.arrows.iter().find(|a| a.id == ARROW).unwrap()
}
fn length(doc: &Document) -> f32 {
    let a = arrow(doc);
    a.start.distance(a.end)
}
fn side(doc: &Document, id: u64) -> Option<Side> {
    doc.arrow_labels
        .iter()
        .find(|l| l.annotation == id)
        .map(|l| l.side)
}
fn assert_centred(doc: &Document, id: u64) {
    let a = arrow(doc);
    let mid = a.start.lerp(a.end, 0.5);
    let c = centre(text(doc, id));
    assert!((c.x - mid.x).abs() < 1e-3, "{c:?} vs {mid:?}");
}

#[test]
fn typed_text_in_a_slot_attaches_centred_and_widens_the_arrow() {
    let mut doc = scheme();
    let wide = "Pd(PPh3)4, K2CO3, toluene/H2O, 80 °C, 12 h";
    let above = type_text(&mut doc, wide, Point::new(40., -12.));
    let below = type_text(&mut doc, "92%", Point::new(70., 14.));
    assert_eq!(side(&doc, above), Some(Side::Above));
    assert_eq!(side(&doc, below), Some(Side::Below));
    assert_centred(&doc, above);
    assert_centred(&doc, below);
    let (w, h) = text(&doc, above).size();
    let pad = doc.drawing_style.bond_length_world * 0.2;
    assert!(w > 120.);
    assert!((length(&doc) - (w + 2. * pad)).abs() < 1e-3);
    // The label clears the arrow on its side.
    assert!(text(&doc, above).position.y + h < 0.);
    assert!(text(&doc, below).position.y > 0.);
    // The arrow grew about its midpoint.
    let a = arrow(&doc);
    assert!((a.start.lerp(a.end, 0.5).x - 60.).abs() < 1e-3);
}

#[test]
fn unattached_text_keeps_its_existing_behaviour() {
    let mut doc = scheme();
    let far = type_text(&mut doc, "note", Point::new(60., -200.));
    let beside = type_text(&mut doc, "+", Point::new(-60., 0.));
    assert!(doc.arrow_labels.is_empty());
    assert_eq!(length(&doc), 120.);
    // Text already sitting in a slot is not captured by unrelated edits.
    let mut legacy = scheme();
    legacy.annotations.push(Annotation {
        id: 9,
        position: Point::new(50., -20.),
        text: "heat".into(),
        format: TextFormat::default(),
    });
    edit(&mut legacy, |doc| {
        doc.add_atom("C", Point::new(300., 0.));
    });
    assert!(legacy.arrow_labels.is_empty());
    let _ = (far, beside);
}

#[test]
fn labels_follow_the_arrow_and_shortening_stops_at_the_label() {
    let mut doc = scheme();
    let id = type_text(&mut doc, "H2, Pd/C, EtOH", Point::new(60., -14.));
    let offset = {
        let t = text(&doc, id);
        t.position.y - arrow(&doc).start.y
    };
    edit(&mut doc, |doc| doc.translate(&[ARROW], 30., 50.));
    assert_centred(&doc, id);
    assert!((text(&doc, id).position.y - arrow(&doc).start.y - offset).abs() < 1e-3);

    let minimum = minimum_length(&doc, ARROW).unwrap();
    let start = arrow(&doc).start;
    edit(&mut doc, |doc| {
        doc.arrows[0].end = start.offset(5., 0.);
    });
    assert_eq!(arrow(&doc).start, start, "the undragged end stays");
    assert!((length(&doc) - minimum).abs() < 1e-3);
    assert_centred(&doc, id);

    // Lengthening by hand is unrestricted.
    edit(&mut doc, |doc| {
        doc.arrows[0].end = start.offset(400., 0.);
    });
    assert_eq!(length(&doc), 400.);
    assert_centred(&doc, id);
}

#[test]
fn editing_text_resyncs_a_fitted_arrow_but_keeps_a_lengthened_one() {
    let mut doc = scheme();
    let id = type_text(
        &mut doc,
        "NaBH4, MeOH, 0 °C to rt, 2 h",
        Point::new(60., -12.),
    );
    let fitted = length(&doc);
    assert!(fitted > 120.);
    let set_text = |doc: &mut Document, value: &str| {
        edit(doc, |doc| {
            doc.annotations
                .iter_mut()
                .find(|a| a.id == id)
                .unwrap()
                .text = value.into();
        })
    };
    set_text(
        &mut doc,
        "NaBH4, MeOH, 0 °C to rt, 2 h, then aq. NH4Cl workup",
    );
    assert!(length(&doc) > fitted);
    assert_centred(&doc, id);
    set_text(&mut doc, "NaBH4");
    let floor = doc.drawing_style.bond_length_world * (2.8 + 0.4);
    assert!((length(&doc) - floor).abs() < 1e-3);
    assert_centred(&doc, id);

    // A longer arrow chosen by the user only grows.
    edit(&mut doc, |doc| {
        let start = doc.arrows[0].start;
        doc.arrows[0].end = start.offset(300., 0.);
    });
    set_text(&mut doc, "LiAlH4");
    assert_eq!(length(&doc), 300.);
}

#[test]
fn dragging_text_away_detaches_and_across_changes_side() {
    let mut doc = scheme();
    let id = type_text(&mut doc, "hν", Point::new(60., -12.));
    let position = text(&doc, id).position;
    // A small nudge re-centres.
    edit(&mut doc, |doc| doc.translate(&[id], 10., -3.));
    assert_eq!(side(&doc, id), Some(Side::Above));
    assert_eq!(text(&doc, id).position, position);
    // Across the arrow.
    edit(&mut doc, |doc| doc.translate(&[id], 0., 70.));
    assert_eq!(side(&doc, id), Some(Side::Below));
    assert_centred(&doc, id);
    // Away from it.
    edit(&mut doc, |doc| doc.translate(&[id], 0., 300.));
    assert_eq!(side(&doc, id), None);
    assert!(text(&doc, id).position.y > 250.);
    // The detached arrow can be shortened freely.
    edit(&mut doc, |doc| doc.arrows[0].end = Point::new(10., 0.));
    assert_eq!(length(&doc), 10.);
}

#[test]
fn wide_text_started_mid_arrow_attaches() {
    let mut doc = scheme();
    let wide = "Pd(PPh3)4, K2CO3, toluene/H2O, 80 °C";
    let width = Annotation {
        id: 0,
        position: Point::default(),
        text: wide.into(),
        format: TextFormat::default(),
    }
    .size()
    .0;
    // The text tool places the top-left corner at the click.
    let id = type_text(&mut doc, wide, Point::new(60. + width / 2., -12.));
    assert_eq!(side(&doc, id), Some(Side::Above));
    assert_centred(&doc, id);
}

#[test]
fn an_occupied_slot_leaves_new_text_unattached() {
    let mut doc = scheme();
    let first = type_text(&mut doc, "a", Point::new(60., -12.));
    let second = type_text(&mut doc, "b", Point::new(30., -12.));
    assert_eq!(side(&doc, first), Some(Side::Above));
    assert_eq!(side(&doc, second), None);
}

#[test]
fn vertical_arrows_take_labels_on_the_left_and_right() {
    let mut doc = Document::default();
    doc.arrows.push(Arrow::new(
        ARROW,
        Point::new(0., 0.),
        Point::new(0., 120.),
        Preset::Forward,
        ArrowStyle::preset(Preset::Forward),
    ));
    let left = type_text(&mut doc, "KOH", Point::new(-15., 60.));
    let right = type_text(&mut doc, "EtOH", Point::new(15., 60.));
    assert_eq!(side(&doc, left), Some(Side::Above));
    assert_eq!(side(&doc, right), Some(Side::Below));
    assert!(centre(text(&doc, left)).x < 0.);
    assert!(centre(text(&doc, right)).x > 0.);
    assert!((centre(text(&doc, left)).y - 60.).abs() < 1e-3);
}

#[test]
fn bending_or_deleting_the_arrow_releases_text() {
    let mut doc = scheme();
    let id = type_text(&mut doc, "Δ", Point::new(60., -12.));
    let mut bent = doc.clone();
    edit(&mut bent, |doc| {
        doc.arrows[0].control = Some(Point::new(60., 40.))
    });
    assert!(bent.arrow_labels.is_empty());
    assert!(bent.annotations.iter().any(|a| a.id == id));
    edit(&mut doc, |doc| doc.delete(&[ARROW]));
    assert!(doc.arrow_labels.is_empty());
    assert!(doc.annotations.iter().any(|a| a.id == id));
}

#[test]
fn straightened_arrows_keep_their_labels() {
    let mut doc = scheme();
    let id = type_text(&mut doc, "Pd/C, H2, EtOAc, rt", Point::new(60., -12.));
    edit(&mut doc, |doc| {
        doc.arrows[0].control = Some(Point::new(60., 40.));
    });
    assert!(doc.arrow_labels.is_empty());
    edit(&mut doc, |doc| doc.arrows[0].straighten());
    edit(&mut doc, |doc| doc.translate(&[id], 0., -2.));
    assert_eq!(side(&doc, id), Some(Side::Above));
    let a = arrow(&doc);
    assert_eq!(a.control, Some(a.start.lerp(a.end, 0.5)));
    assert!(supports(a));
}

#[test]
fn deleting_the_text_removes_the_link() {
    let mut doc = scheme();
    let id = type_text(&mut doc, "Δ", Point::new(60., -12.));
    edit(&mut doc, |doc| doc.delete(&[id]));
    assert!(doc.arrow_labels.is_empty());
    edit(&mut doc, |doc| doc.arrows[0].end = Point::new(10., 0.));
    assert_eq!(length(&doc), 10.);
}

#[test]
fn native_files_and_copies_keep_labels() {
    let mut doc = scheme();
    let above = type_text(
        &mut doc,
        "Pd/C, H2 (1 atm), EtOAc, rt",
        Point::new(60., -12.),
    );
    let below = type_text(&mut doc, "quant.", Point::new(60., 14.));
    let bytes = doc.file_json().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["version"], crate::document::VERSION);
    assert_eq!(json["arrow_labels"][0]["side"], "above");
    let reopened = Document::from_native_file(&bytes).unwrap();
    assert_eq!(reopened.arrow_labels, doc.arrow_labels);
    assert_eq!(reopened.annotations, doc.annotations);
    assert_eq!(reopened.arrows, doc.arrows);

    // Copying the arrow alone copies its labels under new IDs.
    let part = crate::editing::selection(&doc, &[ARROW]);
    assert_eq!(part.arrow_labels.len(), 2);
    let mut pasted = doc.clone();
    let ids = crate::editing::append(&mut pasted, &part, Point::new(0., 100.));
    assert_eq!(ids.len(), 3);
    assert_eq!(pasted.arrow_labels.len(), 4);
    for label in &pasted.arrow_labels[2..] {
        assert!(ids.contains(&label.arrow) && ids.contains(&label.annotation));
    }
    pasted.validate().unwrap();

    // Copying the text alone yields ordinary text.
    let part = crate::editing::selection(&doc, &[below]);
    assert!(part.arrow_labels.is_empty());
    assert_eq!(part.annotations.len(), 1);
    let _ = above;
}

#[test]
fn invalid_links_are_rejected_on_open() {
    let mut doc = scheme();
    let id = type_text(&mut doc, "Δ", Point::new(60., -12.));
    let mut duplicate = doc.clone();
    duplicate.arrow_labels.push(ArrowLabel {
        arrow: ARROW,
        annotation: id,
        side: Side::Below,
    });
    assert!(duplicate.validate().is_err());
    let mut dangling = doc.clone();
    dangling.arrow_labels[0].arrow = 99;
    assert!(Document::from_json(&serde_json::to_vec(&dangling).unwrap()).is_err());
}

#[test]
fn cdxml_notice_names_the_omitted_links() {
    let mut doc = scheme();
    assert_eq!(export_notice(&doc), None);
    type_text(&mut doc, "Δ", Point::new(60., -12.));
    assert_eq!(export_notice(&doc).as_deref(), Some(EXPORT_NOTICE));
}

#[test]
fn an_arrow_grown_to_its_text_follows_it_back_down() {
    let mut doc = scheme();
    doc.arrows[0].end = Point::new(20., 0.);
    let id = type_text(&mut doc, "MeOH", Point::new(10., -12.));
    let grown = length(&doc);
    assert!((grown - minimum_length(&doc, ARROW).unwrap()).abs() < 1e-3);
    edit(&mut doc, |doc| {
        doc.annotations
            .iter_mut()
            .find(|a| a.id == id)
            .unwrap()
            .text = "rt".into();
    });
    assert!(length(&doc) < grown);
    assert!((length(&doc) - minimum_length(&doc, ARROW).unwrap()).abs() < 1e-3);
}

#[test]
fn editing_text_already_beside_an_arrow_leaves_it_in_place() {
    let mut doc = scheme();
    doc.annotations.push(Annotation {
        id: 9,
        position: Point::new(40., -45.),
        text: "heat".into(),
        format: TextFormat::default(),
    });
    let before = doc.annotations[0].position;
    edit(&mut doc, |doc| doc.annotations[0].text = "heat, 2 h".into());
    assert!(doc.arrow_labels.is_empty());
    assert_eq!(doc.annotations[0].position, before);
    // Moving it attaches.
    edit(&mut doc, |doc| doc.translate(&[9], 1., 0.));
    assert_eq!(side(&doc, 9), Some(Side::Above));
}

#[test]
fn delete_and_cut_take_owned_text_with_the_arrow() {
    let mut doc = scheme();
    let id = type_text(&mut doc, "Δ", Point::new(60., -12.));
    let free = type_text(&mut doc, "note", Point::new(60., -400.));
    let ids = with_labels(&doc, &[ARROW]);
    assert!(ids.contains(&id) && !ids.contains(&free));
    assert_eq!(with_labels(&doc, &[id]), vec![id]);
}

#[test]
fn dragging_onto_an_occupied_side_keeps_the_label_already_there() {
    let mut doc = scheme();
    let above = type_text(&mut doc, "a", Point::new(60., -12.));
    let below = type_text(&mut doc, "b", Point::new(60., 14.));
    edit(&mut doc, |doc| doc.translate(&[above], 0., 70.));
    assert_eq!(side(&doc, below), Some(Side::Below));
    assert_eq!(side(&doc, above), None);
}

#[test]
fn collapsing_an_end_onto_the_other_stops_at_the_label() {
    let mut doc = scheme();
    let id = type_text(&mut doc, "H2O", Point::new(60., -12.));
    let start = arrow(&doc).start;
    edit(&mut doc, |doc| doc.arrows[0].end = start);
    assert_eq!(side(&doc, id), Some(Side::Above));
    assert_eq!(arrow(&doc).start, start);
    assert!(arrow(&doc).end.x > start.x);
    assert!((length(&doc) - minimum_length(&doc, ARROW).unwrap()).abs() < 1e-3);
}
