use reshiki::{
    document::{Document, Point},
    editing,
    engine::{LocalEngine, Request},
    graphics::{GraphicKind, GraphicStyle},
    scientific::{self, Drawing, MarkKind, OrbitalKind, Phase, SymbolKind},
};
fn p(x: f32, y: f32) -> Point {
    Point::new(x, y)
}
fn drawing(kind: GraphicKind) -> Drawing {
    Drawing {
        kind,
        style: GraphicStyle::default(),
        phase: Phase::Solid,
        flipped: false,
        attach: true,
    }
}

#[test]
fn all_orbital_phases_and_symbols_survive_native_and_vector_export() {
    let mut doc = Document::default();
    for (row, phase) in Phase::ALL.iter().copied().enumerate() {
        for (col, kind) in OrbitalKind::ALL.iter().copied().enumerate() {
            let mut tool = drawing(GraphicKind::Orbital(kind));
            tool.phase = phase;
            tool.place(
                &mut doc,
                p(col as f32 * 130., row as f32 * 130.),
                p(col as f32 * 130., row as f32 * 130. - 42.),
                false,
                2.,
            )
            .unwrap();
            let graphic = doc.graphics.last().unwrap();
            assert_eq!(graphic.style.width_pt, 0.6);
            assert!(!graphic.commands().is_empty());
            assert_eq!(
                graphic.parts().iter().any(|part| part.filled),
                phase != Phase::Open
            );
            for part in graphic.parts() {
                for point in reshiki::graphics::flattened(&part.commands)
                    .into_iter()
                    .flatten()
                {
                    let (lo, hi) = graphic.bounds();
                    assert!(
                        point.x >= lo.x && point.x <= hi.x && point.y >= lo.y && point.y <= hi.y
                    );
                }
            }
        }
    }
    for (i, kind) in SymbolKind::ALL.iter().copied().enumerate() {
        drawing(GraphicKind::Symbol(kind))
            .place(
                &mut doc,
                p(i as f32 * 45., 420.),
                p(i as f32 * 45., 420.),
                false,
                2.,
            )
            .unwrap();
    }
    doc.validate().unwrap();
    let json = serde_json::to_string(&doc).unwrap();
    assert_eq!(serde_json::from_str::<Document>(&json).unwrap(), doc);
    let svg = reshiki::scene::svg(&doc);
    assert!(svg.contains("rgb(170,170,170)"));
    assert!(
        reshiki::export::drawing(&doc, "pdf")
            .unwrap()
            .starts_with(b"%PDF")
    );
    let p_orbital = &doc.graphics[3];
    assert!(p_orbital.hit(p(390., -25.), 1.));
    assert!(!p_orbital.hit(p(440., 0.), 1.));
}

#[test]
fn chemical_marks_follow_atoms_through_move_copy_rotation_and_validation() {
    let mut doc = Document::default();
    let id = doc.add_atom("N", p(10., 20.));
    scientific::attach(
        doc.atom_mut(id).unwrap(),
        SymbolKind::CirclePlus,
        p(20., -20.),
    )
    .unwrap();
    scientific::attach(doc.atom_mut(id).unwrap(), SymbolKind::LonePair, p(-20., 0.)).unwrap();
    doc.translate(&[id], 40., 10.);
    assert_eq!(doc.atoms[0].marks[0].offset, p(20., -20.));
    let source = doc.clone();
    let copied = editing::append(&mut doc, &source, p(100., 0.));
    assert_eq!(doc.atoms[1].marks, source.atoms[0].marks);
    let pivot = doc.atoms[1].position;
    editing::transform_about(&mut doc, &copied, pivot, 1., 90.);
    assert!(doc.atoms[1].marks[0].offset.distance(p(20., 20.)) < 0.001);
    assert_eq!(doc.atoms[1].charge, 1);
    let (lo, hi) = reshiki::scene::selection_bounds(&doc, &copied).unwrap();
    assert!(lo.x < pivot.x && hi.x > pivot.x + 20.);
    doc.validate().unwrap();
    doc.atoms[1].marks[0].angle = f32::NAN;
    assert!(doc.validate().is_err());
}

#[test]
fn failed_attachment_is_atomic_and_combined_marks_track_two_radicals() {
    let mut doc = Document::default();
    let id = doc.add_atom("C", p(0., 0.));
    let atom = doc.atom_mut(id).unwrap();
    for i in 0..12 {
        scientific::attach(atom, SymbolKind::LonePair, p(i as f32, 0.)).unwrap();
    }
    let before = atom.clone();
    assert!(scientific::attach(atom, SymbolKind::RadicalCation, p(0., 0.)).is_err());
    assert_eq!(*atom, before);
    atom.marks.clear();
    scientific::attach(atom, SymbolKind::RadicalCation, p(20., -20.)).unwrap();
    let one = scientific::mark_parts(atom)
        .iter()
        .filter(|p| p.filled)
        .count();
    atom.radical_electrons = 2;
    assert_eq!(
        scientific::mark_parts(atom)
            .iter()
            .filter(|p| p.filled)
            .count(),
        one + 1
    );
    assert_eq!(atom.marks[0].kind, MarkKind::RadicalIon);
}

#[tokio::test]
async fn attached_charges_and_radicals_recalculate_hydrogens_and_exchange() {
    let engine = LocalEngine::default();
    let mut doc = engine
        .request(Request::import_smiles("[NH4+]"))
        .await
        .unwrap()
        .document
        .unwrap();
    let tool = drawing(GraphicKind::Symbol(SymbolKind::CircleMinus));
    let pos = doc.atoms[0].position;
    tool.place(&mut doc, pos, pos, false, 5.).unwrap();
    let neutral = engine
        .request(Request::molecule("analyze", doc))
        .await
        .unwrap();
    assert_eq!(neutral.analysis.unwrap().formula, "H3N");
    let mut doc = neutral.document.unwrap();
    drawing(GraphicKind::Symbol(SymbolKind::Radical))
        .place(&mut doc, pos, pos, false, 5.)
        .unwrap();
    let result = engine
        .request(Request::molecule("analyze", doc))
        .await
        .unwrap();
    let analysis = result.analysis.unwrap();
    assert_eq!(analysis.unpaired_electrons, 1);
    assert_eq!(analysis.formula, "H2N");
    let doc = result.document.unwrap();
    let mut request = Request::molecule("export", doc.clone());
    request.format = Some("cdxml".into());
    let xml = engine.request(request).await.unwrap().output.unwrap();
    assert!(xml.contains("represent"));
    let result = engine
        .request(Request::import("cdxml", &xml))
        .await
        .unwrap();
    assert_eq!(result.analysis.unwrap().smiles, "[NH2]");
    let mark = &result.document.unwrap().atoms[0].marks[0];
    assert_eq!(mark.kind, MarkKind::Radical);
    assert!(mark.offset.distance(doc.atoms[0].marks[1].offset) < 0.01);
}

#[tokio::test]
async fn mixed_phase_graphics_export_as_grouped_colored_vectors() {
    let engine = LocalEngine::default();
    let mut doc = Document::default();
    let mut tool = drawing(GraphicKind::Orbital(OrbitalKind::Dxy));
    tool.style.stroke = [32, 80, 145];
    tool.place(&mut doc, p(0., 0.), p(60., 0.), false, 2.)
        .unwrap();
    let mut request = Request::molecule("export", doc);
    request.format = Some("cdxml".into());
    let xml = engine.request(request).await.unwrap().output.unwrap();
    let doc = engine
        .request(Request::import("cdxml", &xml))
        .await
        .unwrap()
        .document
        .unwrap();
    assert_eq!(doc.graphics.len(), 4);
    assert_eq!(
        doc.graphics
            .iter()
            .filter(|g| g.style.fill.is_some())
            .count(),
        2
    );
    assert!(doc.graphics.iter().all(|g| g.style.stroke == [32, 80, 145]));
    assert!(!doc.groups.is_empty());
    doc.validate().unwrap();
}
