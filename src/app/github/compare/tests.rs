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
    assert_eq!(rows[0].title, "OH added");
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
fn a_ring_drawn_again_and_edited_says_it_was_matched_by_structure() {
    let (left, ring, _) = scheme();
    // Deleted and drawn again, as an import does: new ids, and an oxygen.
    let right = edited(&left, |doc| {
        doc.delete(&ring);
        let benzene = reshiki::rings::Preset::Benzene.document(42., false);
        let ids = reshiki::editing::append(doc, &benzene, Point::new(300., 0.));
        let carbon = ids.into_iter().find(|id| doc.atom(*id).is_some()).unwrap();
        let at = doc.atom(carbon).unwrap().position;
        let oxygen = doc.add_atom("O", at.offset(0., -40.));
        doc.add_bond(carbon, oxygen, 1, "plain");
    });
    let compare = compared(&left, &right);
    let rows = compare.listed();
    assert_eq!(rows[0].tag, Tag::Chem);
    assert_eq!(rows[0].title, "OH added");
    assert_eq!(
        rows[0].detail,
        "Different chemistry · matched by structure (ids changed)"
    );
    let shown = compare.shown.as_ref().unwrap();
    let painted = (shown.marks.lined.iter())
        .filter(|m| m.paint == ComparePaint::Chemical)
        .count();
    // The oxygen, its carbon and their bond, not the whole ring.
    assert_eq!(painted, 3);
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
    let (at_left, at_right) = (on_left.unwrap().outline, on_right.unwrap().outline);
    // Lined up, the condition is pointed at where the left side has it.
    assert!(at_left.0.distance(lined.unwrap().outline.0) < 0.5);
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
        reading(&sides, false),
        "Changes read from version (left) to your drawing (right)"
    );
    assert_eq!(
        reading(&sides, true),
        "Changes read from version (top) to your drawing (bottom)"
    );
    let github = [
        Side::new("On GitHub", "@alice · 14:05 · add heat"),
        Side::new("Your drawing", "not sent"),
    ];
    assert_eq!(
        reading(&github, false),
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

/// Rings at each place: in a row, as wide as a reaction scheme, or a square.
fn rings(at: &[(f32, f32)]) -> Document {
    let ring = reshiki::rings::Preset::Benzene.document(42., false);
    let mut doc = Document::default();
    for (x, y) in at {
        reshiki::editing::append(&mut doc, &ring, Point::new(*x, *y));
    }
    load(&doc)
}

fn wide() -> Document {
    rings(&[(0., 0.), (400., 0.), (800., 0.), (1200., 0.)])
}

fn square() -> Document {
    rings(&[(0., 0.), (300., 0.), (0., 300.), (300., 300.)])
}

/// An oxygen on the first atom.
fn with_oxygen(doc: &Document) -> Document {
    edited(doc, |doc| {
        let carbon = doc.atoms[0].id;
        let at = doc.atoms[0].position;
        let oxygen = doc.add_atom("O", at.offset(0., -40.));
        doc.add_bond(carbon, oxygen, 1, "plain");
    })
}

#[test]
fn a_wide_drawing_stacks_the_panes_when_that_draws_it_larger() {
    let pane = |doc: &Document| extent(doc);
    let area = Size::new(700., 420.);
    assert!(stacks(area, pane(&wide()), HEADER));
    assert!(!stacks(area, pane(&square()), HEADER));
    // Small enough to draw at full size either way: nothing gained.
    let ring = load(&reshiki::rings::Preset::Benzene.document(42., false));
    assert!(!stacks(area, pane(&ring), HEADER));
}

#[test]
fn a_selected_change_is_zoomed_to_in_both_panes_at_one_scale() {
    let at = |x0, y0, x1, y1| (Point::new(x0, y0), Point::new(x1, y1));
    let spot = |context| {
        Some(Spot {
            outline: context,
            context,
        })
    };
    let frame = at(0., 0., 60., 20.);
    let [left, right] = zoomed(
        [spot(at(0., 0., 10., 10.)), spot(at(30., 0., 70., 4.))],
        frame,
    )
    .unwrap();
    let size = |(lo, hi): (Point, Point)| (hi.x - lo.x, hi.y - lo.y);
    assert_eq!(size(left), (40., 10.));
    assert_eq!(size(left), size(right));
    assert_eq!(left.0, Point::new(-15., 0.));
    assert_eq!(right.0, Point::new(30., -3.));
    // At most twice as large as the whole drawing: a lone atom keeps half
    // the drawing around it.
    let [left, _] = zoomed([spot(at(5., 5., 5., 5.)), None], at(0., 0., 400., 100.)).unwrap();
    assert_eq!(size(left), (200., 50.));
    // Never smaller than the whole drawing: a padded molecule that fills
    // it keeps its size.
    let [left, _] = zoomed([spot(at(-10., -10., 70., 30.)), None], frame).unwrap();
    assert_eq!(size(left), size(frame));
    // An added molecule: the left pane looks where the right has it.
    let [left, right] = zoomed([None, spot(at(5., 5., 15., 25.))], frame).unwrap();
    assert_eq!(left, right);
    assert_eq!(zoomed([None, None], frame), None);
}

#[test]
fn a_selected_chemical_change_zooms_to_its_molecule_and_outlines_its_atoms() {
    let (left, ring, note) = scheme();
    let right = changed(&left, &ring, note);
    let compare = compared(&left, &right);
    let shown = compare.shown.as_ref().unwrap();
    let chem = (shown.report.changes.iter())
        .position(|change| change.kind == Kind::Chemical)
        .unwrap();
    let spot = shown.spots[chem][1].unwrap();
    let (ring_lo, ring_hi) = reshiki::scene::selection_bounds(&right, &ring).unwrap();
    // The outline is the oxygen and its carbon; the zoom keeps the ring.
    assert!(spot.outline.1.x - spot.outline.0.x < ring_hi.x - ring_lo.x);
    let pad = right.drawing_style.bond_length_world / 2.;
    assert!(spot.context.0.x <= ring_lo.x - pad + 0.01);
    assert!(spot.context.1.x >= ring_hi.x + pad - 0.01);
}

#[test]
fn a_chemical_change_is_where_its_painted_atoms_are_not_its_whole_molecule() {
    let (left, ring, note) = scheme();
    let right = changed(&left, &ring, note);
    let compare = compared(&left, &right);
    let shown = compare.shown.as_ref().unwrap();
    let chem = (shown.report.changes.iter())
        .position(|change| change.kind == Kind::Chemical)
        .unwrap();
    let carbon = right.atom(ring[0]).unwrap().position;
    let oxygen = right.atoms.iter().find(|a| a.element == "O").unwrap();
    let (lo, hi) = shown.spots[chem][1].unwrap().outline;
    // The oxygen and its carbon, not the ring of six.
    for p in [carbon, oxygen.position] {
        assert!(
            lo.x <= p.x + 0.01 && p.x <= hi.x + 0.01 && lo.y <= p.y + 0.01 && p.y <= hi.y + 0.01
        );
    }
    let (ring_lo, ring_hi) = reshiki::scene::selection_bounds(&right, &ring).unwrap();
    assert!(hi.y - lo.y < ring_hi.y - ring_lo.y);
    assert!(hi.x - lo.x < ring_hi.x - ring_lo.x);
}

#[test]
fn smiles_breaks_only_between_tokens() {
    let smiles = "C[C@@H](Cl)c1ccc(Br)cc1C%12CC%12N->[Cu]C%(123)";
    let broken = breakable(smiles);
    assert_eq!(broken.replace(['\u{200B}', '\u{2060}'], ""), smiles);
    let tokens: Vec<String> = broken
        .split('\u{200B}')
        .map(|token| token.replace('\u{2060}', ""))
        .collect();
    assert_eq!(
        tokens,
        [
            "C", "[C@@H]", "(", "Cl", ")", "c1", "c", "c", "c", "(", "Br", ")", "c", "c1", "C%12",
            "C", "C%12", "N", "->", "[Cu]", "C%(123)"
        ]
    );
}

#[test]
fn a_change_painted_on_one_side_only_is_found_on_the_other_by_its_atom() {
    // A sulfide oxidised: on the left nothing changed, so the left looks at
    // the sulfur the oxygen is on there, not the whole molecule.
    let mut doc = Document::default();
    let chain: Vec<u64> = (0..5)
        .map(|i| {
            let element = if i == 2 { "S" } else { "C" };
            doc.add_atom(element, Point::new(36. * i as f32, 21. * (i % 2) as f32))
        })
        .collect();
    for pair in chain.windows(2) {
        doc.add_bond(pair[0], pair[1], 1, "plain");
    }
    let left = load(&doc);
    let sulfur = chain[2];
    let right = edited(&left, |doc| {
        let at = doc.atom(sulfur).unwrap().position;
        let oxygen = doc.add_atom("O", at.offset(0., -40.));
        doc.add_bond(sulfur, oxygen, 2, "plain");
    });
    let compare = compared(&left, &right);
    let shown = compare.shown.as_ref().unwrap();
    let chem = (shown.report.changes.iter())
        .position(|change| change.kind == Kind::Chemical)
        .unwrap();
    // The sulfur's label, not the chain.
    let (lo, hi) = shown.spots[chem][0].unwrap().outline;
    assert_eq!(
        (lo, hi),
        reshiki::scene::selection_bounds(&left, &[sulfur]).unwrap()
    );
}

/// A view laid out in a window of `size`, with its accessible controls.
async fn laid_out(
    mut view: Element<'_, Message>,
    size: Size,
) -> (
    iced::advanced::layout::Node,
    reshiki::accessibility::Snapshot,
) {
    use iced::advanced::{Layout, renderer::Headless, widget::Tree, widget::operation};
    let renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .unwrap();
    let mut tree = Tree::new(view.as_widget());
    let node = view.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &iced::advanced::layout::Limits::new(Size::ZERO, size),
    );
    let viewport = iced::Rectangle::with_size(size);
    let mut collect = reshiki::accessibility::Collect::new(viewport);
    view.as_widget_mut().operate(
        &mut tree,
        Layout::new(&node),
        &renderer,
        &mut operation::black_box(&mut collect),
    );
    (node, collect.snapshot().clone())
}

#[tokio::test]
#[ignore = "Opt-in real renderer layout check"]
async fn rendered_compare_stacks_a_wide_scheme_and_keeps_the_badge_by_the_title() {
    let size = Size::new(1000., 560.);
    let pane = |snapshot: &reshiki::accessibility::Snapshot, id: &str| {
        (snapshot.nodes.iter())
            .find(|node| node.id == id)
            .unwrap()
            .bounds
    };
    let doc = wide();
    let right = edited(&doc, |doc| doc.translate(&doc.all_ids(), 0., 30.));
    let compare = compared(&doc, &right);
    let (node, snapshot) = laid_out(compare.dialog("Saved 14:02"), size).await;
    let (top, bottom) = (
        pane(&snapshot, "github-compare-compared"),
        pane(&snapshot, "github-compare-current"),
    );
    assert!(bottom.y >= top.y + top.height, "{top:?} {bottom:?}");
    assert!((top.x - bottom.x).abs() < 1.);
    // The verdict sits by the title, not at the far edge.
    let title = &node.children()[0];
    let [words, badge] = [0, 1].map(|i| title.children()[i].bounds());
    assert!(
        badge.x - (words.x + words.width) < 12.,
        "{words:?} {badge:?}"
    );

    let compare = compared(&square(), &with_oxygen(&square()));
    let (_, snapshot) = laid_out(compare.dialog("Saved 14:02"), size).await;
    let (left, right) = (
        pane(&snapshot, "github-compare-compared"),
        pane(&snapshot, "github-compare-current"),
    );
    assert!(right.x >= left.x + left.width, "{left:?} {right:?}");
    // A row is named with its SMILES as written, no break marks in it.
    let row = (snapshot.nodes.iter())
        .find(|node| node.id == "github-compare-row-0")
        .unwrap();
    assert_eq!(row.name, "CHEM: OH added, c1ccccc1 → Oc1ccccc1");
}

/// A chiral molecule drawn by hand: the wedge carries the stereo.
fn chiral() -> Document {
    let mut doc = Document::default();
    let c = doc.add_atom("C", Point::new(0., 0.));
    let ends = [("C", -36., 21.), ("O", 36., 21.), ("C", 0., -42.)];
    let mut ids = vec![];
    for (element, x, y) in ends {
        let id = doc.add_atom(element, Point::new(x, y));
        ids.push(id);
    }
    doc.add_bond(c, ids[0], 1, "plain");
    doc.add_bond(c, ids[1], 1, "plain");
    doc.add_bond(c, ids[2], 1, "wedge");
    let end = doc.add_atom("C", Point::new(36., -63.));
    doc.add_bond(ids[2], end, 1, "plain");
    load(&doc)
}

#[test]
fn a_flipped_molecule_reads_flipped_and_keeps_its_configuration() {
    let left = chiral();
    let right = edited(&left, |doc| {
        reshiki::editing::transform(
            doc,
            &doc.all_ids(),
            reshiki::editing::Transform::FlipHorizontal,
        )
    });
    let compare = compared(&left, &right);
    assert_eq!(compare.verdict(), Some(Verdict::Same));
    let rows = compare.listed();
    assert_eq!(rows[0].tag, Tag::Mirrored);
    assert_eq!(rows[0].title, "Molecule flipped left–right");
    assert_eq!(rows[0].detail, "Wedges swapped; same configuration");
    assert_eq!(Tag::Mirrored.label(), "MIRRORED");
    assert_eq!(Tag::Mirrored.colors(false), Tag::Moved.colors(false));
    // Positions mirrored, wedge kept: the mirror image.
    let mirrored = edited(&left, |doc| {
        for atom in &mut doc.atoms {
            atom.position.x = -atom.position.x;
        }
    });
    let compare = compared(&left, &mirrored);
    let titles: Vec<&str> = compare.listed().iter().map(|r| r.title.as_str()).collect();
    assert_eq!(
        titles[..2],
        [
            "Mirror image: every stereocenter inverted",
            "Molecule flipped left–right"
        ]
    );
}

#[test]
fn a_scaled_scheme_reads_as_one_line_with_its_scale() {
    let (left, ..) = scheme();
    let right = edited(&left, |doc| {
        reshiki::editing::transform_about(doc, &doc.all_ids(), Point::new(0., 0.), 1.2, 0.)
    });
    let compare = compared(&left, &right);
    let rows = compare.listed();
    assert_eq!(rows[0].title, "Whole scheme moved");
    assert!(
        rows[0].detail.contains("scaled to 120%"),
        "{}",
        rows[0].detail
    );
    // One molecule scaled about itself.
    let ring: Vec<u64> = left.atoms.iter().take(6).map(|a| a.id).collect();
    let right = edited(&left, |doc| {
        let at = doc.atom(ring[0]).unwrap().position;
        reshiki::editing::transform_about(doc, &ring, at, 0.8, 0.)
    });
    let compare = compared(&left, &right);
    let row = &compare.listed()[0];
    assert_eq!(
        (row.tag, row.title.as_str()),
        (Tag::Scaled, "Molecule scaled to 80%")
    );
}

#[test]
fn parts_read_with_their_atom_numbers() {
    let part = |kind, what: &str, at: &[&str]| Part {
        kind,
        what: what.into(),
        at: at.iter().map(|s| (*s).to_owned()).collect(),
    };
    let parts = [
        part(PartKind::Added, "OH", &["C4"]),
        part(PartKind::Replaced, "Cl → Br", &["C2"]),
        part(PartKind::Bond, "single → double", &["C1", "C2"]),
        part(PartKind::Inverted, "", &["C3"]),
    ];
    assert_eq!(parts_title(&parts[..1]), "OH added at C4");
    assert_eq!(parts_title(&parts[..2]), "OH added at C4, Cl → Br at C2");
    assert_eq!(
        parts_title(&parts),
        "OH added at C4, Cl → Br at C2, +2 more"
    );
    assert_eq!(part_text(&parts[2]), "C1–C2 single → double");
    assert_eq!(part_text(&parts[3]), "inverted at C3");
    assert_eq!(
        part_text(&part(PartKind::BondAdded, "", &["C1", "C3"])),
        "C1–C3 bond added"
    );
    assert_eq!(
        parts_title(&[part(PartKind::BondRemoved, "", &[])]),
        "Bond removed"
    );
    assert_eq!(
        parts_title(&[part(PartKind::Removed, "NO₂", &[])]),
        "NO₂ removed"
    );
    assert_eq!(
        parts_title(&[part(PartKind::Inverted, "", &[])]),
        "Stereocenter inverted"
    );
}

/// A benzene ring with a halogen on its first atom.
fn halide(element: &str) -> Document {
    let ring = reshiki::rings::Preset::Benzene.document(42., false);
    let mut doc = Document::default();
    reshiki::editing::append(&mut doc, &ring, Point::new(0., 0.));
    let carbon = doc.atoms[0].id;
    let at = doc.atoms[0].position;
    let halogen = doc.add_atom(element, at.offset(0., -42.));
    doc.add_bond(carbon, halogen, 1, "plain");
    load(&doc)
}

#[test]
fn a_change_on_a_small_molecule_never_zooms_out_and_outlines_its_whole_label() {
    let left = halide("Cl");
    let halogen = left.atoms.iter().find(|a| a.element == "Cl").unwrap().id;
    let right = edited(&left, |doc| {
        doc.atoms
            .iter_mut()
            .find(|a| a.id == halogen)
            .unwrap()
            .element = "Br".into();
    });
    let compare = compared(&left, &right);
    let shown = compare.shown.as_ref().unwrap();
    let chem = (shown.report.changes.iter())
        .position(|change| change.kind == Kind::Chemical)
        .unwrap();
    let spots = &shown.spots[chem];
    let frame = shown.lined_frame;
    let [left_frame, right_frame] = zoomed([spots[0], spots[2]], frame).unwrap();
    // A narrow pane draws the whole molecule below full size: the change
    // must not draw it smaller still.
    let pane = Size::new(150., 300.);
    for zoom in [left_frame, right_frame].map(|frame| compare_zoom(pane, frame)) {
        assert!(zoom >= compare_zoom(pane, frame) - 1e-4, "{zoom}");
    }
    // The outline goes around the Br label, not through it.
    let (lo, hi) = spots[1].unwrap().outline;
    let (label_lo, label_hi) = reshiki::scene::selection_bounds(&right, &[halogen]).unwrap();
    assert!(label_hi.x - label_lo.x > 5.);
    assert!(
        lo.x <= label_lo.x && lo.y <= label_lo.y,
        "{lo:?} {label_lo:?}"
    );
    assert!(
        hi.x >= label_hi.x && hi.y >= label_hi.y,
        "{hi:?} {label_hi:?}"
    );
}

#[test]
fn the_same_row_wraps_its_smiles_like_a_chemical_change() {
    let left = chiral();
    let right = edited(&left, |doc| {
        reshiki::editing::transform(
            doc,
            &doc.all_ids(),
            reshiki::editing::Transform::FlipHorizontal,
        )
    });
    let compare = compared(&left, &right);
    let same = (compare.listed().into_iter())
        .find(|row| row.tag == Tag::Same)
        .unwrap();
    // As SMILES, so it breaks between tokens instead of running off the edge.
    assert!(same.detail.is_empty(), "{}", same.detail);
    assert!(same.smiles.as_deref().is_some_and(|s| s.contains("C")));
}
