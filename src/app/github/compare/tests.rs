use super::*;
use reshiki::document::{Annotation, Point};

/// A drawing as read from a file, the way both sides of a compare are read.
fn load(doc: &Document) -> Document {
    Document::from_native_file(&doc.file_json().unwrap()).unwrap()
}

/// Two rings and a condition under a label: the left side of each test, and
/// the ids of the second ring.
fn scheme() -> (Document, Vec<u64>, u64) {
    let ring = reshiki::rings::Preset::Benzene.document(42., false);
    let mut doc = Document::default();
    reshiki::editing::append(&mut doc, &ring, Point::new(0., 0.));
    let second = reshiki::editing::append(&mut doc, &ring, Point::new(300., 0.));
    let second = second
        .into_iter()
        .filter(|id| doc.atom(*id).is_some())
        .collect();
    let note = doc.next_id();
    doc.annotations.push(Annotation {
        id: note,
        position: Point::new(150., -60.),
        text: "H2SO4".into(),
        format: Default::default(),
    });
    (load(&doc), second, note)
}

fn edited(doc: &Document, edit: impl FnOnce(&mut Document)) -> Document {
    let mut doc = doc.clone();
    edit(&mut doc);
    load(&doc)
}

/// Everything moved, an oxygen on the second ring and the condition edited.
fn changed(left: &Document, ring: &[u64], note: u64) -> Document {
    edited(left, |doc| {
        doc.translate(&doc.all_ids(), -60., 25.);
        let carbon = ring[0];
        let at = doc.atom(carbon).unwrap().position;
        let oxygen = doc.add_atom("O", at.offset(0., -40.));
        doc.add_bond(carbon, oxygen, 1, "plain");
        let found = doc.annotations.iter_mut().find(|n| n.id == note).unwrap();
        found.text = "H2SO4, heat".into();
    })
}

/// The compare once its comparison has landed, as the app runs it.
fn compared(left: &Document, right: &Document) -> Compare {
    let mut compare = Compare::new(Ok(left.clone()), right.clone());
    let shown = Arc::new(build(left, right));
    compare.update(Action::Done(compare.generation, Some(shown)));
    compare
}

#[test]
fn it_opens_on_everything_lined_up_and_says_it_is_comparing() {
    let (left, ..) = scheme();
    let compare = Compare::new(Ok(left.clone()), left.clone());
    assert_eq!(compare.mode, Mode::Everything);
    assert!(compare.line_up);
    assert_eq!(compare.verdict(), Some(Verdict::Comparing));
    assert_eq!(compare.count(Mode::Everything), None);
    assert!(compare.listed().is_empty());
    let compare = compared(&left, &left);
    assert_eq!(compare.verdict(), Some(Verdict::Same));
    assert_eq!(compare.count(Mode::Everything), Some(0));
}

#[test]
fn each_mode_counts_and_lists_its_own_changes() {
    let (left, ring, note) = scheme();
    let right = changed(&left, &ring, note);
    let mut compare = compared(&left, &right);
    assert_eq!(compare.verdict(), Some(Verdict::Changed(1)));
    assert_eq!(compare.count(Mode::Chemistry), Some(1));
    // The move and the condition.
    assert_eq!(compare.count(Mode::Layout), Some(2));
    assert_eq!(compare.count(Mode::Everything), Some(3));
    let tags = |compare: &Compare| {
        compare
            .listed()
            .iter()
            .map(|row| row.tag)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        tags(&compare),
        [Tag::Chem, Tag::Text, Tag::Moved, Tag::Same]
    );
    let rows = compare.listed();
    assert_eq!(rows[0].title, "O added");
    assert_eq!(rows[0].smiles.as_deref(), Some("c1ccccc1 → Oc1ccccc1"));
    assert_eq!(rows[1].title, "Text: “H2SO4” → “H2SO4, heat”");
    assert_eq!(rows[2].title, "Whole scheme moved");
    assert!(
        rows[2].detail.starts_with("0.7 cm left, 0.3 cm down"),
        "{}",
        rows[2].detail
    );
    assert_eq!(rows[3].title, "1 molecule unchanged");
    compare.update(Action::Mode(Mode::Chemistry));
    assert_eq!(tags(&compare), [Tag::Chem, Tag::Same]);
    compare.update(Action::Mode(Mode::Layout));
    assert_eq!(tags(&compare), [Tag::Text, Tag::Moved, Tag::Same]);
    // The marks follow the mode: a move is an outline, not paint.
    let shown = compare.shown.as_ref().unwrap();
    let painted = |paint| {
        shown
            .marks
            .lined
            .iter()
            .filter(|m| m.paint == paint)
            .count()
    };
    assert_eq!(painted(ComparePaint::Moved), 1);
    // The oxygen, its carbon and their bond.
    assert_eq!(painted(ComparePaint::Chemical), 3);
}

#[test]
fn lined_up_the_right_side_lies_on_the_left_and_unticked_it_is_where_it_is() {
    let (left, ..) = scheme();
    let right = edited(&left, |doc| doc.translate(&doc.all_ids(), 80., -40.));
    let mut compare = compared(&left, &right);
    for atom in &left.atoms {
        let there = compare.right_shown().atom(atom.id).unwrap().position;
        assert!(atom.position.distance(there) < 0.01);
    }
    compare.update(Action::LineUp(false));
    assert_eq!(compare.right_shown(), &right);
    // Either way the drawings themselves stay as they are.
    assert_eq!(compare.left.as_ref(), Ok(&left));
    assert_eq!(compare.right, right);
}

#[test]
fn a_clicked_line_points_at_its_objects_in_both_drawings() {
    let (left, ring, note) = scheme();
    let right = changed(&left, &ring, note);
    let mut compare = compared(&left, &right);
    let text = compare
        .listed()
        .iter()
        .find(|row| row.tag == Tag::Text)
        .and_then(|row| row.change)
        .unwrap();
    compare.update(Action::Select(text));
    assert_eq!(compare.selected, Some(text));
    let shown = compare.shown.clone().unwrap();
    let [on_left, on_right, lined] = shown.spots[text];
    let (at_left, at_right) = (on_left.unwrap(), on_right.unwrap());
    // Lined up, the condition is pointed at where the left side has it.
    assert!(at_left.0.distance(lined.unwrap().0) < 0.5);
    assert!(at_left.0.distance(at_right.0) > 50.);
    // Again lets go; another mode starts with nothing pointed at.
    compare.update(Action::Select(text));
    assert_eq!(compare.selected, None);
    compare.update(Action::Select(text));
    compare.update(Action::Mode(Mode::Chemistry));
    assert_eq!(compare.selected, None);
}

#[test]
fn a_late_result_of_an_older_comparison_is_dropped() {
    let (left, ring, note) = scheme();
    let right = changed(&left, &ring, note);
    let older = Compare::new(Ok(left.clone()), right.clone());
    let mut newer = Compare::new(Ok(left.clone()), left.clone());
    assert_ne!(older.generation, newer.generation);
    let stale = Arc::new(build(&left, &right));
    newer.update(Action::Done(older.generation, Some(stale)));
    assert!(newer.shown.is_none(), "still comparing");
    newer.update(Action::Done(
        newer.generation,
        Some(Arc::new(build(&left, &left))),
    ));
    assert_eq!(newer.verdict(), Some(Verdict::Same));
}

#[test]
fn a_drawing_deleted_on_github_is_not_compared() {
    let (left, ..) = scheme();
    let compare = Compare::new(Err("Deleted on GitHub".into()), left);
    assert_eq!(compare.verdict(), None);
    assert!(compare.listed().is_empty());
}

#[test]
fn the_users_own_highlights_stay_and_are_not_a_chemical_change() {
    let (plain, ..) = scheme();
    let lit = edited(&plain, |doc| {
        doc.atoms[0].display.highlight = Some(reshiki::palette::Color::Ink);
    });
    let compare = compared(&plain, &lit);
    assert_eq!(compare.verdict(), Some(Verdict::Same));
    assert_eq!(compare.right_shown(), &lit, "lined up, nothing moved");
    assert_eq!(compare.count(Mode::Layout), Some(1), "a style line");
}

#[test]
fn a_comparison_that_stops_says_so() {
    let (left, ..) = scheme();
    let mut compare = Compare::new(Ok(left.clone()), left);
    compare.update(Action::Done(compare.generation, None));
    assert_eq!(compare.verdict(), Some(Verdict::Failed));
}

#[test]
fn the_compared_version_is_on_the_left_and_the_drawing_now_on_the_right() {
    assert_eq!(SIDES, [Role::Compared, Role::Current]);
    assert_eq!(SIDES.map(Role::pill), ["COMPARED", "CURRENT"]);
    let [version, now] = history_sides("Saved 14:02");
    assert_eq!(
        version.spoken(Role::Compared),
        "Compared: Version · saved 14:02"
    );
    assert_eq!(
        now.spoken(Role::Current),
        "Current: Your drawing · open now"
    );
    let sides = history_sides("Esterification · @alice · 02:02");
    assert_eq!(sides[0].meta, "Esterification · @alice · 02:02");
    assert_eq!(
        reading(&sides),
        "Changes read from version (left) to your drawing (right)"
    );
    let github = [
        Side::new("On GitHub", "@alice · 14:05 · add heat"),
        Side::new("Your drawing", "not sent"),
    ];
    assert_eq!(
        reading(&github),
        "Changes read from on GitHub (left) to your drawing (right)"
    );
    assert_eq!(
        github[0].spoken(Role::Compared),
        "Compared: On GitHub · @alice · 14:05 · add heat"
    );
    // The drawing now is the right side compared, and the left is the version.
    let (left, ..) = scheme();
    let right = edited(&left, |doc| doc.translate(&doc.all_ids(), 5., 0.));
    let compare = Compare::new(Ok(left.clone()), right.clone());
    assert_eq!(compare.left.as_ref(), Ok(&left));
    assert_eq!(compare.right, right);
}
