//! Issue #376 evidence: type each label on a terminal carbon of ethane in
//! Automatic mode, as the atom label editor's Apply does, and report what
//! it makes. Usage: ev376 OUT_DIR
use reshiki_model::atom_text::{Mode, apply, description};
use reshiki_model::document::{Document, Point};

fn main() {
    let out = std::env::args().nth(1).expect("OUT_DIR");
    for typed in ["morpholino", "2-thienyl", "MeO", "NC"] {
        let mut doc = Document::default();
        let methyl = doc.add_atom("C", Point::new(0., 0.));
        let endpoint = doc.add_atom("C", Point::new(42., 0.));
        doc.add_bond(methyl, endpoint, 1, "plain");
        println!("== {typed}");
        println!("editor description: {}", description(typed, &[], Mode::Auto));
        println!(
            "editor suggestion: {:?}",
            reshiki_model::atom_text::suggestion(typed, Mode::Auto)
        );
        match apply(&doc, endpoint, typed, Mode::Auto) {
            Err(error) => println!("Apply: ERROR {error}"),
            Ok(result) => {
                match result.abbreviation(endpoint) {
                    Some(g) => println!(
                        "Apply: group label={:?} reverse_label={:?} members={}",
                        g.label,
                        g.reverse_label,
                        g.members.len()
                    ),
                    None => {
                        let a = result.atom(endpoint).unwrap();
                        println!(
                            "Apply: atom element={:?} variable={:?}",
                            a.element, a.display.variable
                        );
                    }
                }
                let path = format!("{out}/{typed}.rsk");
                std::fs::write(&path, result.file_json().unwrap()).unwrap();
                println!("saved: {typed}.rsk");
            }
        }
    }
}
