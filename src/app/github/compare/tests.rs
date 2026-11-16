use super::*;
use reshiki::document::{Annotation, Point};

/// A drawing as read from a file, the way both sides of a compare are read.
fn load(doc: &Document) -> Document {
    Document::from_native_file(&doc.file_json().unwrap()).unwrap()
}

fn ring() -> Document {
    load(&reshiki::rings::Preset::Benzene.document(42., false))
}

fn edited(doc: &Document, edit: impl FnOnce(&mut Document)) -> Document {
    let mut doc = doc.clone();
    edit(&mut doc);
    load(&doc)
}

fn key(a: u64, b: u64) -> (u64, u64) {
    (a.min(b), a.max(b))
}

fn bonds_of(doc: &Document, id: u64) -> HashSet<(u64, u64)> {
    doc.bonds
        .iter()
        .filter(|bond| bond.a == id || bond.b == id)
        .map(|bond| key(bond.a, bond.b))
        .collect()
}

#[test]
fn ids_survive_saving_and_reading() {
    let doc = ring();
    let again = load(&doc);
    assert_eq!(doc.all_ids(), again.all_ids());
    assert_eq!(diff(&doc, &again), Diff::default());
}

#[test]
fn a_moved_atom_and_its_bonds_are_marked_on_both_sides() {
    let left = ring();
    let id = left.atoms[0].id;
    let right = edited(&left, |doc| doc.atoms[0].position.x += 10.);
    let found = diff(&left, &right);
    let bonds = bonds_of(&left, id);
    assert_eq!(bonds.len(), 2);
    for side in [&found.left, &found.right] {
        assert_eq!(side.ids, HashSet::from([id]));
        assert_eq!(side.bonds, bonds);
    }
    assert!(!found.settings);
}

#[test]
fn an_added_atom_is_marked_only_where_it_is() {
    let left = ring();
    let from = left.atoms[0].id;
    let mut added = 0;
    let right = edited(&left, |doc| {
        added = doc.add_atom("O", Point::new(200., 200.));
        doc.add_bond(from, added, 1, "plain");
    });
    let found = diff(&left, &right);
    assert_eq!(found.left, Marks::default());
    assert_eq!(found.right.ids, HashSet::from([added]));
    assert_eq!(found.right.bonds, HashSet::from([key(from, added)]));
}

#[test]
fn a_reused_id_holding_another_atom_is_marked_on_both_sides() {
    let base = ring();
    let last = base.atoms.last().unwrap().id;
    // Each side deleted the last atom and drew a new one, which took its id.
    let side = |element: &str, x: f32| {
        edited(&base, |doc| {
            doc.delete(&[last]);
            let id = doc.add_atom(element, Point::new(x, 300.));
            assert_eq!(id, last);
        })
    };
    let (left, right) = (side("N", 100.), side("S", 400.));
    let found = diff(&left, &right);
    assert!(found.left.ids.contains(&last));
    assert!(found.right.ids.contains(&last));
}

#[test]
fn a_bond_order_change_marks_only_that_bond() {
    let left = ring();
    let bond = left
        .bonds
        .iter()
        .find(|bond| bond.order == 1)
        .map(|bond| key(bond.a, bond.b))
        .unwrap();
    let right = edited(&left, |doc| {
        let found = doc
            .bonds
            .iter_mut()
            .find(|b| key(b.a, b.b) == bond)
            .unwrap();
        found.order = 2;
    });
    let found = diff(&left, &right);
    for side in [&found.left, &found.right] {
        assert!(side.ids.is_empty());
        assert_eq!(side.bonds, HashSet::from([bond]));
    }
}

#[test]
fn changed_text_is_boxed() {
    let mut left = ring();
    let id = left.next_id();
    left.annotations.push(Annotation {
        id,
        position: Point::new(0., 80.),
        text: "Step 1".into(),
        format: Default::default(),
    });
    let left = load(&left);
    let right = edited(&left, |doc| doc.annotations[0].text = "Step 2".into());
    let found = diff(&left, &right);
    assert_eq!(found.right.ids, HashSet::from([id]));
    assert_eq!(boxes(&right, &found.right).len(), 1);
}

#[test]
fn style_theme_and_label_settings_are_caught_without_a_field_list() {
    let base = ring();
    let ring_atoms: Vec<u64> = base.atoms.iter().map(|atom| atom.id).collect();
    type Edit = Box<dyn Fn(&mut Document)>;
    let edits: [(&str, Edit); 3] = [
        (
            "style",
            Box::new(|doc| doc.drawing_style.line_width_pt *= 2.),
        ),
        (
            "atom labels",
            Box::new(|doc| doc.atom_labels.carbons = reshiki::atom_labels::Carbons::All),
        ),
        (
            "ring fill",
            Box::new(move |doc| {
                doc.ring_fills.push(reshiki::ring_fills::RingFill {
                    atoms: ring_atoms.clone(),
                    color: reshiki::palette::Color::Ink,
                });
            }),
        ),
    ];
    for (what, edit) in edits {
        let other = edited(&base, |doc| edit(doc));
        assert_ne!(base, other, "{what}: the edit must survive saving");
        let found = diff(&base, &other);
        assert!(found.settings, "{what}");
        assert!(
            found.left.ids.is_empty() && found.right.ids.is_empty(),
            "{what}"
        );
    }
}

#[test]
fn a_highlight_added_or_removed_is_boxed_and_both_drawings_stay_as_they_are() {
    let plain = ring();
    let id = plain.atoms[0].id;
    let lit = edited(&plain, |doc| {
        doc.atoms[0].display.highlight = Some(reshiki::palette::Color::Ink);
    });
    for (left, right) in [(&plain, &lit), (&lit, &plain)] {
        let compare = Compare::new(Ok(left.clone()), right.clone());
        assert_eq!(compare.left.as_ref(), Ok(left), "drawn unchanged");
        assert_eq!(&compare.right, right, "drawn unchanged");
        assert_eq!(compare.marks[0].len(), 1);
        assert_eq!(compare.marks[1].len(), 1);
        let (lo, hi) = compare.marks[1][0];
        let at = right.atom(id).unwrap().position;
        assert!(lo.x < at.x && at.x < hi.x && lo.y < at.y && at.y < hi.y);
    }
}

#[test]
fn a_drawing_compared_with_nothing_marks_nothing() {
    let compare = Compare::new(Err("Deleted on GitHub".into()), ring());
    assert_eq!(compare.marks, [Vec::new(), Vec::new()]);
    assert!(!compare.settings);
}
