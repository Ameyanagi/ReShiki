use super::*;

fn label_ring_intersections(doc: &Document) -> anyhow::Result<Vec<(u64, u64, u64)>> {
    use anyhow::Context;
    let mut intersections = Vec::new();
    for atom in &doc.atoms {
        // Selection bounds include blank font ascender space. Check the
        // positioned glyph ink used by rendering and bond clipping instead.
        for (lo, hi) in crate::scene::atom_label_ink_boxes(atom, doc) {
            for bond in doc
                .bonds
                .iter()
                .filter(|b| b.projection && b.a != atom.id && b.b != atom.id)
            {
                let a = doc.atom(bond.a).context("Ring endpoint")?.position;
                let b = doc.atom(bond.b).context("Ring endpoint")?.position;
                if a.x.max(b.x) < lo.x
                    || a.x.min(b.x) > hi.x
                    || a.y.max(b.y) < lo.y
                    || a.y.min(b.y) > hi.y
                {
                    continue;
                }
                let sides = [lo, Point::new(lo.x, hi.y), hi, Point::new(hi.x, lo.y)]
                    .map(|p| (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x));
                if !sides.iter().all(|v| *v > 0.) && !sides.iter().all(|v| *v < 0.) {
                    intersections.push((atom.id, bond.a, bond.b));
                }
            }
        }
    }
    Ok(intersections)
}

#[test]
fn sugar_labels_do_not_touch_unrelated_ring_edges() -> anyhow::Result<()> {
    for template in templates().map_err(anyhow::Error::msg)? {
        let doc = &template.document;
        let intersections = label_ring_intersections(doc)?;
        assert!(
            intersections.is_empty(),
            "{} label/ring-edge intersections: {intersections:?}",
            template.name
        );
    }
    Ok(())
}

#[test]
fn sugar_label_ring_intersection_check_detects_real_overlap() -> anyhow::Result<()> {
    use anyhow::Context;
    let mut doc = sugar_document(Sugar::Glucose, Anomer::Alpha, 42.).map_err(anyhow::Error::msg)?;
    let a = doc.atom(4).context("Ring endpoint")?.position;
    let b = doc.atom(5).context("Ring endpoint")?.position;
    // Move the C3 hydroxyl onto the unrelated back ring edge. Its visible
    // O glyph must be detected even though empty ascender space is ignored.
    doc.atom_mut(9).context("Hydroxyl oxygen")?.position =
        Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.);
    assert!(label_ring_intersections(&doc)?.contains(&(9, 4, 5)));
    Ok(())
}
