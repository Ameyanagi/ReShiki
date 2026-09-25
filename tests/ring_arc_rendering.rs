use anyhow::Context;
use reshiki::{
    document::{Document, Point},
    graphics::PathCommand,
    scene::{self, Primitive},
};

fn lines(doc: &Document) -> usize {
    scene::primitives(doc)
        .iter()
        .filter(|p| matches!(p, Primitive::Line(..)))
        .count()
}

fn curves(doc: &Document) -> usize {
    scene::primitives(doc)
        .iter()
        .filter(|p| {
            matches!(p, Primitive::Path { commands, .. }
            if commands.iter().any(|c| matches!(c, PathCommand::Cubic(..))))
        })
        .count()
}

fn joined_edges(doc: &Document) -> usize {
    scene::primitives(doc)
        .iter()
        .map(|p| match p {
            Primitive::Path {
                commands,
                filled: true,
                ..
            } => commands
                .iter()
                .filter(|c| matches!(c, PathCommand::Close))
                .count(),
            _ => 0,
        })
        .sum()
}

#[test]
fn partial_aromatic_curve_does_not_add_dashes_on_remaining_ring_edges() -> anyhow::Result<()> {
    for size in [5, 6, 7, 8] {
        for tilt in [0., 60.] {
            for wrap in [false, true] {
                let mut doc = Document::default();
                let mut ids = reshiki::editing::ring(&mut doc, Point::default(), size, true, 0.);
                if wrap {
                    ids.rotate_right(1);
                }
                let bonds = doc.bonds.clone();
                assert!(
                    reshiki::ring_arcs::toggle(&mut doc, &ids[..3]).map_err(anyhow::Error::msg)?
                );
                reshiki::projection::tilt(&mut doc, &ids, tilt, true);
                assert_eq!(curves(&doc), 1, "size={size}, tilt={tilt}, wrap={wrap}");
                assert_eq!(
                    lines(&doc),
                    usize::from(size),
                    "Only outer ring edges should remain"
                );
                assert!(doc.bonds.iter().all(|b| b.order == 4));
                let restored: Document = serde_json::from_slice(&serde_json::to_vec(&doc)?)?;
                assert_eq!(scene::svg(&restored), scene::svg(&doc));
                assert!(
                    !reshiki::ring_arcs::toggle(&mut doc, &ids[..3]).map_err(anyhow::Error::msg)?
                );
                assert_eq!(doc.bonds, bonds, "Curve display must not change chemistry");
                assert_eq!(curves(&doc), 1, "Toggling off restores the full circle");
                assert_eq!(lines(&doc), usize::from(size));
            }
        }
    }
    Ok(())
}

#[test]
fn a_partial_curve_leaves_other_rings_and_acyclic_aromatic_bonds_visible() -> anyhow::Result<()> {
    let mut doc = Document::default();
    let ids = reshiki::editing::ring(&mut doc, Point::default(), 6, true, 0.);
    reshiki::editing::ring(&mut doc, Point::new(200., 0.), 6, true, 0.);
    let a = doc.add_atom("C", Point::new(400., 0.));
    let b = doc.add_atom("C", Point::new(440., 0.));
    doc.add_bond(a, b, 4, "plain");
    reshiki::ring_arcs::toggle(&mut doc, &ids[..3]).map_err(anyhow::Error::msg)?;
    assert_eq!(curves(&doc), 2, "One partial curve and one complete circle");
    assert_eq!(
        lines(&doc),
        12 + 6,
        "Only the acyclic aromatic bond retains five dashes"
    );
    Ok(())
}

#[test]
fn partial_curve_on_a_fused_ring_does_not_add_dashes_or_hide_its_neighbor() -> anyhow::Result<()> {
    let mut doc = Document::default();
    let ids = reshiki::editing::ring(&mut doc, Point::default(), 6, true, 0.);
    let a = doc.atom(ids[0]).context("Missing ring atom")?.position;
    let b = doc.atom(ids[1]).context("Missing ring atom")?.position;
    reshiki::editing::ring(
        &mut doc,
        Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.),
        6,
        true,
        5.,
    );
    assert_eq!(doc.bonds.len(), 11);
    reshiki::ring_arcs::toggle(&mut doc, &ids[2..5]).map_err(anyhow::Error::msg)?;
    assert_eq!(curves(&doc), 2);
    assert_eq!(lines(&doc), 11);
    Ok(())
}

#[test]
fn breaking_the_ring_restores_aromatic_fallback() -> anyhow::Result<()> {
    let mut doc = Document::default();
    let ids = reshiki::editing::ring(&mut doc, Point::default(), 6, true, 0.);
    reshiki::ring_arcs::toggle(&mut doc, &ids[..3]).map_err(anyhow::Error::msg)?;
    doc.bonds.pop().context("Missing ring bond")?;
    assert_eq!(curves(&doc), 0);
    assert_eq!(
        lines(&doc),
        5 * 6,
        "Each open aromatic edge retains its dashed component"
    );
    Ok(())
}

#[test]
fn partial_curve_preserves_unselected_double_bonds_and_figure_exports() -> anyhow::Result<()> {
    let mut doc = Document::default();
    let ids = reshiki::editing::ring(&mut doc, Point::default(), 6, true, 0.);
    for (i, bond) in doc.bonds.iter_mut().enumerate() {
        bond.order = if i % 2 == 0 { 2 } else { 1 };
    }
    for atom in &mut doc.atoms {
        atom.aromatic = false;
    }
    let bonds = doc.bonds.clone();
    reshiki::ring_arcs::toggle(&mut doc, &ids[..3]).map_err(anyhow::Error::msg)?;
    assert_eq!(curves(&doc), 1);
    assert_eq!(
        lines(&doc) + joined_edges(&doc),
        8,
        "Six outer edges and two unselected double-bond strokes"
    );
    for format in ["svg", "png", "pdf"] {
        assert!(
            !reshiki::export::drawing(&doc, format)
                .map_err(anyhow::Error::msg)?
                .is_empty()
        );
    }
    assert!(
        !reshiki::export::clipboard_png(&doc)
            .map_err(anyhow::Error::msg)?
            .is_empty()
    );
    reshiki::ring_arcs::toggle(&mut doc, &ids[..3]).map_err(anyhow::Error::msg)?;
    assert_eq!(doc.bonds, bonds);
    assert_eq!(lines(&doc) + joined_edges(&doc), 9);
    Ok(())
}
