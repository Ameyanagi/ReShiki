use reshiki::{
    canvas_theme::{CanvasTheme, ColorTheme},
    depth_appearance::{self as depth, Paint},
    document::{Document, Point},
    graphics::PathCommand,
    palette::{Color, Hue, Row},
    scene::{self, Primitive},
};

fn chain() -> Document {
    let mut doc = Document::default();
    for (i, z) in [-30., -10., 10., 30.].into_iter().enumerate() {
        let id = doc.add_atom("C", Point::new(i as f32 * 42., 0.));
        doc.atom_mut(id).unwrap().depth = z;
        if i > 0 {
            doc.add_bond(id - 1, id, 1, "plain");
        }
    }
    doc
}

#[test]
fn continuous_paint_is_component_local_offset_invariant_and_flat_is_normal() {
    let mut doc = chain();
    let ids = doc.all_ids();
    let base = doc.clone();
    depth::enable(&mut doc, &ids, 0.6).unwrap();
    assert_eq!(doc.atoms, base.atoms);
    assert_eq!(doc.bonds, base.bonds);
    let paint = Paint::new(&doc);
    let weights: Vec<_> = ids.iter().map(|id| paint.amount(*id)).collect();
    assert!(weights.windows(2).all(|w| w[0] > w[1]));
    assert_eq!(weights.first().copied(), Some(0.6));
    assert_eq!(weights.last().copied(), Some(0.));
    for atom in &mut doc.atoms {
        atom.depth += 100.;
    }
    let shifted = Paint::new(&doc);
    for id in &ids {
        assert_eq!(paint.amount(*id), shifted.amount(*id));
    }
    let other = doc.add_atom("O", Point::new(500., 0.));
    doc.atom_mut(other).unwrap().depth = -5000.;
    let all = doc.all_ids();
    depth::enable(&mut doc, &all, 0.6).unwrap();
    let separate = Paint::new(&doc);
    for id in &ids {
        assert_eq!(paint.amount(*id), separate.amount(*id));
    }
    assert_eq!(separate.amount(other), 0.);
    for atom in &mut doc.atoms {
        atom.depth = 4.;
    }
    let flat = Paint::new(&doc);
    assert!(all.iter().all(|id| flat.amount(*id) == 0.));
    let mut clear = doc.clone();
    clear.depth_appearance.clear();
    assert_eq!(scene::svg(&doc), scene::svg(&clear));
}

#[test]
fn freezing_saves_paint_and_geometry_and_clearing_restores_live_base_colors() {
    let mut doc = chain();
    doc.bonds[0].color = Color::Custom([30, 90, 150]);
    let ids = doc.all_ids();
    depth::enable(&mut doc, &ids, depth::DEFAULT_STRENGTH).unwrap();
    let automatic = scene::svg(&doc);
    let atoms = doc.atoms.clone();
    let bonds = doc.bonds.clone();
    assert_eq!(depth::freeze(&mut doc, &ids), 1);
    assert!(!depth::is_automatic_for(&doc, &ids));
    assert_eq!(doc.atoms, atoms);
    assert_eq!(doc.bonds, bonds);
    assert_eq!(scene::svg(&doc), automatic);
    let mut reopened = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(reopened.depth_appearance, doc.depth_appearance);
    for atom in &mut reopened.atoms {
        atom.depth = -atom.depth;
    }
    assert_eq!(scene::svg(&reopened), automatic);
    reopened.translate(&ids, 20., 15.);
    assert_ne!(scene::svg(&reopened), automatic);
    reopened.bonds[0].color = Color::Custom([180, 30, 50]);
    assert_eq!(depth::clear(&mut reopened, &ids), ids.len());
    assert!(reopened.depth_appearance.is_empty());
    assert_eq!(reopened.bonds[0].color, Color::Custom([180, 30, 50]));
    assert_eq!(reopened.bonds[0].display, "plain");
    assert!(!reopened.bonds[0].projection);
}

#[test]
fn frozen_legacy_front_bonds_keep_their_appearance_and_authority_during_ordinary_tilt() {
    let mut original = Document::default();
    let ids = reshiki::editing::ring(&mut original, Point::default(), 6, true, 0.);
    reshiki::projection::tilt(&mut original, &ids, 35., true);
    reshiki::projection::depth_bonds(&mut original, &ids);
    original.bonds[0].z_order = 3;
    assert!(original.bonds.iter().any(|bond| bond.display == "bold"));
    assert!(original.bonds.iter().any(|bond| bond.display == "plain"));
    assert!(original.bonds.iter().all(|bond| bond.projection));

    // The existing Front bonds command continues to update old drawings that
    // have no new frozen depth-paint scope.
    let mut legacy = original.clone();
    for _ in 0..3 {
        reshiki::projection::tilt(&mut legacy, &ids, 60., true);
    }
    assert!(
        legacy
            .bonds
            .iter()
            .zip(&original.bonds)
            .any(|(after, before)| after.display != before.display)
    );
    assert!(
        legacy
            .bonds
            .iter()
            .all(|bond| bond.projection && bond.order == 4)
    );
    assert_eq!(legacy.bonds[0].z_order, 3);

    let mut frozen = original.clone();
    depth::enable(&mut frozen, &ids, 0.6).unwrap();
    depth::freeze(&mut frozen, &ids);
    let appearance = frozen.depth_appearance.clone();
    let paint = Paint::new(&frozen);
    for _ in 0..3 {
        reshiki::projection::tilt(&mut frozen, &ids, 60., true);
    }
    assert_ne!(
        frozen.atoms, original.atoms,
        "ordinary tilt still edits XYZ"
    );
    assert_eq!(
        frozen.bonds, original.bonds,
        "freezing keeps the projection marker, endpoint authority, orders and styles"
    );
    assert_eq!(frozen.depth_appearance, appearance);
    let after = Paint::new(&frozen);
    for id in &ids {
        assert_eq!(after.amount(*id), paint.amount(*id));
    }
    frozen.validate().unwrap();
}

#[test]
fn custom_colors_labels_appendages_and_tints_use_the_visible_paper_once() {
    for mode in CanvasTheme::ALL {
        let mut doc = chain();
        doc.canvas_theme = mode;
        doc.color_theme = ColorTheme::Jmol;
        let rear = doc.atoms[0].id;
        let atom = doc.atom_mut(rear).unwrap();
        atom.element = "N".into();
        atom.explicit_h = 2;
        atom.no_implicit = true;
        atom.isotope = 15;
        atom.charge = 1;
        atom.display.highlight = Some(Color::Palette(Hue::Blue, Row::Tint));
        atom.display.color_override = true;
        atom.text_style = Some(reshiki::typography::TextStyle {
            color: Color::Custom([20, 80, 140]),
            ..Default::default()
        });
        doc.bonds[0].color = Color::Custom([20, 80, 140]);
        let ids = doc.all_ids();
        depth::enable(&mut doc, &ids, 0.6).unwrap();
        let paint = Paint::new(&doc);
        let expected = paint.color(Color::Custom([20, 80, 140]), 0.6).rgb();
        let parts = scene::primitives(&doc);
        for text in ["N", "H", "2", "15", "+"] {
            assert!(
                parts
                    .iter()
                    .any(|p| matches!(p, Primitive::Text { text: value, color, .. }
                if value == text && mode.color(*color) == expected)),
                "{mode}: {text}"
            );
        }
        let baked = depth::materialize(&doc).into_owned();
        assert!(baked.depth_appearance.is_empty());
        assert_eq!(
            baked
                .atom(rear)
                .unwrap()
                .text_style
                .as_ref()
                .unwrap()
                .color
                .rgb(),
            expected
        );
        assert_eq!(depth::materialize(&baked).as_ref(), &baked);
        assert_eq!(scene::svg(&doc), scene::svg(&baked));
        assert_eq!(doc.bonds[0].color, Color::Custom([20, 80, 140]));
        // Explicit zero overrides expose current base paint without double fading.
        depth::override_fade(&mut doc, &[rear], Some(0.)).unwrap();
        assert_eq!(Paint::new(&doc).amount(rear), 0.);
        assert_eq!(
            depth::materialize(&doc)
                .atom(rear)
                .unwrap()
                .text_style
                .as_ref()
                .unwrap()
                .color
                .rgb(),
            [20, 80, 140]
        );
    }
}

#[test]
fn ring_fill_highlight_and_aromatic_curve_remain_attached_and_share_depth_paint() {
    let mut doc = Document::default();
    let ids = reshiki::editing::ring(&mut doc, Point::default(), 6, true, 0.);
    reshiki::projection::tilt(&mut doc, &ids, 55., true);
    reshiki::ring_fills::apply(&mut doc, &ids, Some(Color::Palette(Hue::Purple, Row::Tint)));
    reshiki::highlights::apply(&mut doc, &ids, Some(Color::Palette(Hue::Amber, Row::Tint)));
    let original = doc.clone();
    depth::enable(&mut doc, &ids, 0.6).unwrap();
    let paint = Paint::new(&doc);
    let baked = paint.materialize(&doc).into_owned();
    assert_eq!(baked.ring_fills[0].atoms, original.ring_fills[0].atoms);
    assert_ne!(baked.ring_fills[0].color, original.ring_fills[0].color);
    assert!(baked.bonds.iter().any(|b| {
        b.highlight
            != original
                .bonds
                .iter()
                .find(|old| old.a == b.a && old.b == b.b)
                .unwrap()
                .highlight
    }));
    // Circles preserve the existing defining edge's paint, so a materialized
    // exchange snapshot renders identically without renderer-only ring state.
    let circle_color = reshiki::aromatic::circles(&baked)[0].color.rgb();
    assert!(scene::primitives(&doc).iter().any(|p| matches!(p,
        Primitive::Path { commands, style, filled: false }
        if style.stroke.rgb() == circle_color && commands.iter().any(|c| matches!(c, PathCommand::Cubic(..))))));
    assert_eq!(scene::svg(&doc), scene::svg(&baked));
    let segment: Vec<_> = ids.iter().take(3).copied().collect();
    reshiki::ring_arcs::toggle(&mut doc, &segment).unwrap();
    assert!(scene::primitives(&doc).iter().any(|p| matches!(p,
        Primitive::Path { commands, style, filled: false }
        if style.stroke.rgb() != [0; 3] && commands.iter().any(|c| matches!(c, PathCommand::Cubic(..))))));
    assert!(
        doc.bonds
            .iter()
            .all(|b| b.order == 4 && b.display == "plain" && !b.projection)
    );
    let centroid = reshiki::projection::add_centroid(&mut doc, &ids).unwrap();
    let metal = doc.add_atom("Fe", Point::new(100., 100.));
    doc.add_bond(centroid, metal, 5, "dashed");
    let paint = Paint::new(&doc);
    assert!((paint.amount(centroid) - paint.mean(&ids)).abs() < 0.0001);
    assert!(depth::materialize(&doc).bonds.last().unwrap().color != Color::Ink);
    doc.validate().unwrap();
}

#[test]
fn partial_copy_and_remapping_preserve_source_paint_and_deleted_atoms_prune() {
    let mut doc = chain();
    let ids = doc.all_ids();
    depth::enable(&mut doc, &ids, 0.6).unwrap();
    depth::override_fade(&mut doc, &ids[1..2], Some(0.35)).unwrap();
    let source = doc.clone();
    let selected = depth::selection(&doc, &ids[..2]);
    assert!(!selected[0].automatic);
    assert_eq!(
        selected[0].weights.get(&ids[0]),
        doc.depth_appearance[0].weights.get(&ids[0])
    );
    let mapping = ids.iter().map(|id| (*id, id + 100)).collect();
    let remapped = depth::remap(&selected, &mapping);
    assert_eq!(remapped[0].atoms, vec![ids[0] + 100, ids[1] + 100]);
    assert_eq!(
        remapped[0].weights.get(&(ids[0] + 100)),
        selected[0].weights.get(&ids[0])
    );
    let full = reshiki::editing::selection(&doc, &ids);
    assert_eq!(full.depth_appearance, doc.depth_appearance);
    let partial = reshiki::editing::selection(&doc, &ids[..2]);
    partial.validate().unwrap();
    assert!(!partial.depth_appearance[0].automatic);
    let source_paint = Paint::new(&doc);
    let partial_paint = Paint::new(&partial);
    for id in &ids[..2] {
        assert_eq!(partial_paint.amount(*id), source_paint.amount(*id));
    }
    let mut destination = Document::default();
    destination.add_atom("O", Point::new(-200., 0.));
    let pasted = reshiki::editing::append(&mut destination, &partial, Point::new(100., 20.));
    assert_eq!(pasted.len(), 2);
    destination.validate().unwrap();
    let pasted_paint = Paint::new(&destination);
    for (source_id, pasted_id) in ids[..2].iter().zip(&pasted) {
        assert_eq!(
            pasted_paint.amount(*pasted_id),
            source_paint.amount(*source_id)
        );
    }
    assert_eq!(
        doc, source,
        "selection and append keep source identity untouched"
    );
    doc.delete(&ids[..1]);
    doc.validate().unwrap();
    assert!(
        doc.depth_appearance
            .iter()
            .all(|s| !s.atoms.contains(&ids[0]))
    );
}

#[test]
fn malformed_scope_is_rejected_without_accepting_invalid_weights_or_ownership() {
    let mut doc = chain();
    let ids = doc.all_ids();
    assert!(depth::enable(&mut doc, &ids, f32::NAN).is_err());
    depth::enable(&mut doc, &ids, 0.6).unwrap();
    for invalid in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
        let mut bad = doc.clone();
        bad.depth_appearance[0].weights.insert(ids[0], invalid);
        assert!(bad.validate().is_err());
    }
    let mut duplicate = doc.clone();
    duplicate
        .depth_appearance
        .push(doc.depth_appearance[0].clone());
    assert!(duplicate.validate().is_err());
    doc.depth_appearance[0].atoms.push(u64::MAX);
    assert!(doc.validate().is_err());
}

#[tokio::test]
async fn paint_preserves_tetrahedral_and_alkene_identity_including_exchange() -> anyhow::Result<()>
{
    use anyhow::{Context, ensure};
    use reshiki::engine::{LocalEngine, Request};
    let engine = LocalEngine::default();
    for smiles in ["F[C@](Cl)(Br)I", "F/C=C/F"] {
        let response = engine
            .request(Request::import_smiles(smiles))
            .await
            .map_err(anyhow::Error::msg)?;
        let reference = response.analysis.context("reference analysis")?.inchikey;
        let mut doc = response.document.context("document")?;
        for (index, atom) in doc.atoms.iter_mut().enumerate() {
            atom.depth = index as f32 * 12.;
        }
        let base_atoms = doc.atoms.clone();
        let base_bonds = doc.bonds.clone();
        let ids = doc.all_ids();
        depth::enable(&mut doc, &ids, 0.6).map_err(anyhow::Error::msg)?;
        depth::freeze(&mut doc, &ids);
        assert_eq!(doc.atoms, base_atoms);
        assert_eq!(doc.bonds, base_bonds);
        let painted = depth::materialize(&doc).into_owned();
        let checked = engine
            .request(Request::molecule("analyze", painted.clone()))
            .await
            .map_err(anyhow::Error::msg)?;
        ensure!(checked.analysis.context("painted analysis")?.inchikey == reference);
        for format in ["cdxml", "cdx"] {
            let mut request = Request::molecule("export", painted.clone());
            request.format = Some(format.into());
            let output = engine
                .request(request)
                .await
                .map_err(anyhow::Error::msg)?
                .output
                .context("export")?;
            let restored = engine
                .request(Request::import(format, &output))
                .await
                .map_err(anyhow::Error::msg)?;
            ensure!(
                restored.analysis.context("round-trip analysis")?.inchikey == reference,
                "{format} changed identity"
            );
        }
        for format in ["svg", "png", "pdf"] {
            ensure!(
                !reshiki::export::drawing(&doc, format)
                    .map_err(anyhow::Error::msg)?
                    .is_empty()
            );
        }
    }
    Ok(())
}

#[test]
fn crossing_cage_keeps_depth_order_and_transparent_gaps_after_painting() {
    let mut doc = Document::default();
    for (x, y, z) in [
        (-60., -40., -10.),
        (60., 40., -10.),
        (-60., 40., 10.),
        (60., -40., 10.),
    ] {
        let id = doc.add_atom("C", Point::new(x, y));
        doc.atom_mut(id).unwrap().depth = z;
    }
    doc.add_bond(1, 2, 1, "plain");
    doc.add_bond(3, 4, 1, "plain");
    doc.add_bond(2, 3, 1, "plain");
    let gaps = reshiki::crossings::gaps(&doc);
    assert_eq!(gaps[0].len(), 1);
    assert!(gaps[1].is_empty());
    let ids = doc.all_ids();
    depth::enable(&mut doc, &ids, 0.6).unwrap();
    let baked = depth::materialize(&doc).into_owned();
    assert_eq!(reshiki::crossings::gaps(&baked)[0].len(), 1);
    assert!(reshiki::crossings::gaps(&baked)[1].is_empty());
    let svg = scene::svg(&doc);
    assert!(!svg.contains("<rect"));
    assert!(!svg.contains("rgb(255,255,255)"));
    doc.bonds[0].z_order = 2;
    assert!(reshiki::crossings::gaps(&doc)[0].is_empty());
    assert_eq!(reshiki::crossings::gaps(&doc)[1].len(), 1);
}
