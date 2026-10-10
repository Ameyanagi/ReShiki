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
        snap_orbitals: true,
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
    tool.style.stroke = reshiki::palette::Color::Custom([32, 80, 145]);
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
    assert!(
        doc.graphics
            .iter()
            .all(|g| g.style.stroke == reshiki::palette::Color::Custom([32, 80, 145]))
    );
    assert!(!doc.groups.is_empty());
    doc.validate().unwrap();
}

#[tokio::test]
async fn palette_colored_orbitals_export_as_shown_on_the_dark_canvas() {
    use reshiki::{
        canvas_theme::{CanvasTheme, ColorTheme},
        palette::{Color, Hue, Palette, Row},
    };
    let engine = LocalEngine::default();
    let mut doc = Document {
        canvas_theme: CanvasTheme::Dark,
        color_theme: ColorTheme::Presentation,
        ..Default::default()
    };
    let mut tool = drawing(GraphicKind::Orbital(OrbitalKind::Dxy));
    tool.style.stroke = Color::Palette(Hue::Blue, Row::Strong);
    tool.place(&mut doc, p(0., 0.), p(60., 0.), false, 2.)
        .unwrap();
    let blue = Palette::of(&doc).swatch(Hue::Blue, Row::Strong);
    let mut request = Request::molecule("export", doc);
    request.format = Some("cdxml".into());
    let xml = engine.request(request).await.unwrap().output.unwrap();
    let imported = engine
        .request(Request::import("cdxml", &xml))
        .await
        .unwrap()
        .document
        .unwrap();
    let orbitals: Vec<_> = imported
        .graphics
        .iter()
        .filter(|g| g.style.stroke != Color::Ink)
        .collect();
    assert!(!orbitals.is_empty());
    assert!(
        orbitals
            .iter()
            .all(|g| g.style.stroke == Color::Custom(blue))
    );
}

fn vector_fill_at(commands: &[reshiki::graphics::PathCommand], point: Point) -> bool {
    reshiki::graphics::flattened(commands).iter().any(|path| {
        let mut inside = false;
        for (a, b) in path
            .iter()
            .zip(path.iter().cycle().skip(1))
            .take(path.len())
        {
            if (a.y > point.y) != (b.y > point.y)
                && point.x < (b.x - a.x) * (point.y - a.y) / (b.y - a.y) + a.x
            {
                inside = !inside;
            }
        }
        inside
    })
}

#[tokio::test]
async fn orbital_label_clearance_survives_editable_vector_exchange() {
    let engine = LocalEngine::default();
    let source = engine
        .request(Request::import_smiles("C[15NH3+]"))
        .await
        .unwrap()
        .document
        .unwrap();
    let n = source
        .atoms
        .iter()
        .find(|atom| atom.element == "N")
        .unwrap()
        .id;
    let center = source.atom(n).unwrap().position;
    for phase in Phase::ALL {
        for format in ["cdxml", "cdx"] {
            let mut doc = source.clone();
            let mut tool = drawing(GraphicKind::Orbital(OrbitalKind::S));
            tool.phase = *phase;
            tool.place(&mut doc, center, center.offset(0., -42.), false, 10.)
                .unwrap();
            doc.graphics[0].layer = 1;
            let before = doc.clone();
            let mut request = Request::molecule("export", doc.clone());
            request.format = Some(format.into());
            let exported = engine.request(request).await.unwrap().output.unwrap();
            let imported = engine
                .request(Request::import(format, &exported))
                .await
                .unwrap()
                .document
                .unwrap();
            let imported_n = imported
                .atoms
                .iter()
                .find(|atom| atom.element == "N")
                .unwrap();
            assert_eq!(imported_n.charge, 1);
            assert_eq!(imported_n.isotope, 15);
            assert!(!imported.graphics.is_empty());
            assert_eq!(
                imported
                    .graphics
                    .iter()
                    .flat_map(|graphic| graphic.parts())
                    .any(|part| part.filled),
                *phase != Phase::Open,
                "Exchange must retain the outline versus filled phase",
            );
            for graphic in &imported.graphics {
                for part in graphic.parts().iter().filter(|part| part.filled) {
                    assert!(
                        !vector_fill_at(&part.commands, imported_n.position),
                        "{phase:?}/{format} orbital fill must retain a transparent gap at N"
                    );
                }
            }
            assert_eq!(
                doc, before,
                "Preparing exports must not move atoms or orbital frames"
            );
            assert!(reshiki::scene::svg(&imported).contains("<text"));
            assert!(
                reshiki::export::drawing(&doc, "pdf")
                    .unwrap()
                    .starts_with(b"%PDF")
            );
            assert!(
                reshiki::export::clipboard_png(&doc)
                    .unwrap()
                    .starts_with(b"\x89PNG")
            );
        }
    }
}

const REAR_ORBITAL_COLOR: reshiki::palette::Color = reshiki::palette::Color::Custom([27, 103, 191]);

fn rear_orbital_fixture() -> (Document, u64) {
    let mut doc = Document::from_native_file(include_bytes!(
        "fixtures/rear-opacity/c60-rear-opacity-25.rsk"
    ))
    .unwrap();
    assert_eq!((doc.atoms.len(), doc.bonds.len()), (60, 90));
    let original = doc.clone();
    let ids = doc.all_ids();
    reshiki::depth_appearance::set_rear_opacity(&mut doc, &ids, 0.).unwrap();
    let paint = reshiki::rear_opacity::Paint::new(&doc);
    let hidden = doc
        .atoms
        .iter()
        .find(|atom| paint.atom(atom.id) == 0.)
        .expect("The authentic cage must have an occluded rear atom")
        .id;
    doc.atom_mut(hidden).unwrap().display.carbons = Some(reshiki::atom_labels::Carbons::All);
    let center = doc.atom(hidden).unwrap().position;
    let mut tool = drawing(GraphicKind::Orbital(OrbitalKind::S));
    tool.snap_orbitals = false;
    tool.style.stroke = REAR_ORBITAL_COLOR;
    tool.place(&mut doc, center, center.offset(0., -42.), false, 10.)
        .unwrap();
    assert_eq!(reshiki::rear_opacity::Paint::new(&doc).atom(hidden), 0.);
    assert_eq!(doc.bonds, original.bonds);
    assert!(!reshiki::transaction::chemistry_changed(&original, &doc));
    doc.validate().unwrap();
    (doc, hidden)
}

fn rear_orbital_scene_paths(doc: &Document) -> Vec<(Vec<reshiki::graphics::PathCommand>, bool)> {
    reshiki::scene::primitives(doc)
        .into_iter()
        .filter_map(|primitive| match primitive {
            reshiki::scene::Primitive::Path {
                commands,
                style,
                filled,
            } if style.stroke == REAR_ORBITAL_COLOR => Some((commands, filled)),
            _ => None,
        })
        .collect()
}

#[test]
fn composed_rear_atom_label_clears_orbital_ink_only_when_it_is_painted() {
    let (source, hidden) = rear_orbital_fixture();
    let center = source.atom(hidden).unwrap().position;
    let raw: Vec<_> = source
        .graphics
        .first()
        .unwrap()
        .parts()
        .into_iter()
        .map(|part| (part.commands, part.filled))
        .collect();
    assert!(
        raw.iter()
            .any(|(path, fill)| *fill && vector_fill_at(path, center))
    );
    for alpha in [0., 0.25, 1.] {
        let mut doc = source.clone();
        let ids: Vec<_> = doc.atoms.iter().map(|atom| atom.id).collect();
        reshiki::depth_appearance::set_rear_opacity(&mut doc, &ids, alpha).unwrap();
        assert_eq!(reshiki::rear_opacity::Paint::new(&doc).atom(hidden), alpha);
        let before = doc.clone();
        let paths = rear_orbital_scene_paths(&doc);
        assert!(!paths.is_empty(), "Must observe actual orbital scene ink");
        let fill_at = |point| {
            paths
                .iter()
                .any(|(path, fill)| *fill && vector_fill_at(path, point))
        };
        assert_eq!(fill_at(center), alpha == 0., "Rear label alpha {alpha}");
        assert!(
            fill_at(center.offset(20., 0.)),
            "Clearance must not erase the orbital"
        );
        if alpha == 0. {
            assert_eq!(
                paths, raw,
                "An invisible label must leave the original vectors exact"
            );
        }
        let svg = reshiki::scene::svg(&doc);
        assert_eq!(svg.contains(">C</text>"), alpha > 0.);
        assert_eq!(
            reshiki::export::clipboard_drawing(&doc, "svg").unwrap(),
            svg.as_bytes(),
            "The public figure route must retain these exact scene vectors"
        );
        assert_eq!(
            doc, before,
            "Rendering must not mutate chemistry or orbital frames"
        );
        assert_eq!(doc.atoms, source.atoms);
        assert_eq!(doc.bonds, source.bonds);
        assert_eq!(doc.graphics, source.graphics);
        let reopened = Document::from_native_file(&doc.file_json().unwrap()).unwrap();
        assert_eq!(reopened, doc.current());
    }
}

#[test]
fn composed_rear_orbital_editable_copy_restores_labels_and_matches_opaque_export() {
    let (source, hidden) = rear_orbital_fixture();
    for alpha in [0., 0.25] {
        let mut doc = source.clone();
        let ids: Vec<_> = doc.atoms.iter().map(|atom| atom.id).collect();
        reshiki::depth_appearance::set_rear_opacity(&mut doc, &ids, alpha).unwrap();
        let before = doc.clone();
        let request = Request::molecule("export", doc.clone());
        let error = reshiki::exchange::drawing::write(&doc, (&request).into())
            .unwrap_err()
            .to_string();
        assert!(error.contains("rear opacity") && error.contains("100%"));
        let (copied, notices) = reshiki::exchange::drawing::write_clipboard(&doc).unwrap();
        assert!(
            notices
                .iter()
                .any(|notice| notice.contains("omits rear opacity"))
        );
        let mut opaque = doc.clone();
        reshiki::depth_appearance::set_rear_opacity(&mut opaque, &ids, 1.).unwrap();
        let request = Request::molecule("export", opaque.clone());
        let reference = reshiki::exchange::drawing::write(&opaque, (&request).into()).unwrap();
        assert_eq!(
            copied, reference,
            "Editable copy must use the restored-label geometry"
        );
        assert!(copied.contains(">C</s>") && copied.contains("<curve"));
        let center = opaque.atom(hidden).unwrap().position;
        let paths = rear_orbital_scene_paths(&opaque);
        assert!(paths.iter().any(|(_, fill)| *fill));
        assert!(
            !paths
                .iter()
                .any(|(path, fill)| *fill && vector_fill_at(path, center))
        );
        assert_eq!(
            doc, before,
            "The lossy external copy must leave the native source exact"
        );
    }
}
