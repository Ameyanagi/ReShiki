//! Issue #376 follow-up: NC and OCN read from the bond. Each is typed in
//! Automatic mode on a terminal carbon of ethane whose bond leaves to the
//! right (left-hand end), to the left (right-hand end) or vertically.
//! The editor text is `WRONG_SIDE` when `wrong_side` holds, as in the app.
use reshiki_model::atom_text::{Mode, WRONG_SIDE, apply, description, wrong_side};
use reshiki_model::document::{Document, Point};

fn main() {
    for typed in ["NC", "OCN", "MeO"] {
        for (end, x, y) in [("left-hand end", -42., 0.), ("right-hand end", 42., 0.), ("vertical bond", 0., 42.)] {
            let mut doc = Document::default();
            let methyl = doc.add_atom("C", Point::new(0., 0.));
            let endpoint = doc.add_atom("C", Point::new(x, y));
            doc.add_bond(methyl, endpoint, 1, "plain");
            println!("== {typed} on the {end}");
            let editor = if wrong_side(&doc, endpoint, typed) { WRONG_SIDE } else { description(typed, &[], Mode::Auto) };
            println!("editor description: {editor}");
            let result = apply(&doc, endpoint, typed, Mode::Auto).unwrap();
            match result.abbreviation(endpoint) {
                Some(g) => println!("Apply: group label={:?} reverse_label={:?}", g.label, g.reverse_label),
                None => {
                    let a = result.atom(endpoint).unwrap();
                    println!("Apply: atom element={:?} variable={:?}", a.element, a.display.variable);
                }
            }
        }
    }
}
