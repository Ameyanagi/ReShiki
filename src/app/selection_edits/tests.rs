use crate::app::{App, Message};
use reshiki::{document::Point, palette::Color};

/// Invert selection refreshes the same controls as any other selection
/// change, the bond color included.
#[test]
fn invert_selection_shows_the_newly_selected_bonds_color() {
    let (mut app, _) = App::new();
    app.tab.doc = Default::default();
    let a = app.tab.doc.add_atom("C", Point::default());
    let b = app.tab.doc.add_atom("O", Point::new(42., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    app.tab.doc.bonds[0].color = Color::Custom([18, 52, 86]);
    app.set_selection(Vec::new());
    assert_ne!(app.tab.bond_color_input, "#123456");

    let _ = app.update(Message::InvertSelection);

    assert_eq!(app.tab.selected, vec![a, b]);
    assert_eq!(app.tab.bond_color_input, "#123456");
}
