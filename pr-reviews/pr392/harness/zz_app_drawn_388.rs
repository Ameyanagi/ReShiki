//! Temporary harness for #388 evidence: builds a drawing through the app's own
//! symbol placement (scientific::Drawing::place, as used by place_graphic).
use reshiki::{document::{Document, Point}, graphics::{GraphicKind, GraphicStyle}, scientific::{Drawing, Phase, SymbolKind}};
#[test]
fn app_drawn() {
    let mut doc = Document::default();
    let len = doc.drawing_style.bond_length_world;
    for (row, (element, kind)) in [("O", SymbolKind::CircleMinus), ("N", SymbolKind::CirclePlus)].into_iter().enumerate() {
        let y = row as f32 * len * 3.;
        let c = doc.add_atom("C", Point::new(0., y));
        let x = doc.add_atom(element, Point::new(len, y));
        doc.add_bond(c, x, 1, "plain");
        let at = doc.atom(x).unwrap().position;
        let tool = Drawing { kind: GraphicKind::Symbol(kind), style: GraphicStyle::default(), phase: Phase::Solid, flipped: false, attach: true, snap_orbitals: true };
        tool.place(&mut doc, at, at, false, 5.).unwrap();
        assert_eq!(doc.atom(x).unwrap().marks[0].size_pt, None);
    }
    std::fs::write(std::env::var("OUT").unwrap(), doc.file_json().unwrap()).unwrap();
}
