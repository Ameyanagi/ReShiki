//! Issue #379 evidence: methoxide lone pair -> acetone carbonyl carbon,
//! attached curved arrow, then the end tangent square aimed toward the C=O
//! axis, committed the way the app commits a handle edit, then exported to
//! PNG with reshiki-io's own figure renderer.
use reshiki_model::{
    arrow_anchors::{self, Pick},
    arrows::{ArrowStyle, Preset},
    document::{Document, Point},
    scientific::{self, SymbolKind},
};

fn main() {
    let out = std::env::args().nth(1).expect("Usage: aim379 OUT_DIR");
    let mut d = Document::default();
    // Acetone, C=O up.
    let c = d.add_atom("C", Point::new(0., 0.));
    for (degrees, symbol, order) in [(270f32, "O", 2), (150., "C", 1), (30., "C", 1)] {
        let (s, k) = degrees.to_radians().sin_cos();
        let n = d.add_atom(symbol, Point::new(42. * k, 42. * s));
        d.add_bond(c, n, order, "plain");
    }
    // Methoxide to the left, lone pair on the side facing acetone.
    let o = d.add_atom("O", Point::new(-120., 0.));
    d.atom_mut(o).unwrap().charge = -1;
    let me = d.add_atom("C", Point::new(-120. - 42. * 0.866, -21.));
    d.add_bond(me, o, 1, "plain");
    scientific::attach(
        d.atom_mut(o).unwrap(),
        SymbolKind::LonePair,
        Point::new(25., 0.),
    )
    .unwrap();
    let source = arrow_anchors::pick(&d, Point::new(-95., 0.), 2.).unwrap();
    let id = arrow_anchors::create(
        &mut d,
        &source,
        &Pick::Atom(c),
        Preset::Curved,
        ArrowStyle::preset(Preset::Curved),
    )
    .unwrap();
    let write = |doc: &Document, name: &str| {
        let png = reshiki_io::export::drawing(doc, "png").unwrap();
        std::fs::write(format!("{out}/{name}.png"), png).unwrap();
    };
    write(&d, "default");
    // Drag the arrival square 20 units from the end, 235 degrees (up-left,
    // toward the C=O axis), then commit as the app does.
    let end = d.arrows.iter().find(|a| a.id == id).unwrap().end;
    let (s, k) = 235f32.to_radians().sin_cos();
    let p = end.offset(20. * k, 20. * s);
    let before = d.clone();
    //AIM
    let _ = reshiki_model::transaction::reconcile(&mut d, before).unwrap();
    let a = d.arrows.iter().find(|a| a.id == id).unwrap();
    println!(
        "end {:?} arrival control {:?} end_anchor {:?}",
        a.end,
        a.cubic.unwrap()[1],
        a.end_anchor
    );
    write(&d, "aimed");
}
