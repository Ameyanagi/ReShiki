use super::*;

#[test]
fn recognized_haworth_atoms_include_ring_and_direct_substituents() -> anyhow::Result<()> {
    for template in templates().map_err(anyhow::Error::msg)? {
        let mut doc = template.document;
        let ring: std::collections::HashSet<_> = doc
            .bonds
            .iter()
            .filter(|bond| bond.projection)
            .flat_map(|bond| [bond.a, bond.b])
            .collect();
        let mut expected = ring.clone();
        for bond in &doc.bonds {
            if ring.contains(&bond.a) || ring.contains(&bond.b) {
                expected.extend([bond.a, bond.b]);
            }
        }
        let outside = doc.add_atom("N", Point::new(300., 0.));
        let recognized = interchange::recognized_atoms(&doc);
        assert_eq!(recognized, expected, "{}", template.name);
        assert!(!recognized.contains(&outside));

        doc.atoms[0].position.x += 20.;
        assert!(interchange::recognized_atoms(&doc).is_empty());
    }
    Ok(())
}

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

/// The horizontal extent of an atom's label ink.
fn ink_span(doc: &Document, id: u64) -> anyhow::Result<(f32, f32)> {
    use anyhow::Context;
    let boxes = crate::scene::atom_label_ink_boxes(doc.atom(id).context("Label atom")?, doc);
    anyhow::ensure!(!boxes.is_empty(), "atom {id} has no label ink");
    Ok(boxes.iter().fold((f32::MAX, f32::MIN), |(lo, hi), (a, b)| {
        (lo.min(a.x), hi.max(b.x))
    }))
}

/// Applies typed text to an atom and returns its group's drawn text and alignment.
fn typed_group(
    doc: &Document,
    id: u64,
    text: &str,
) -> anyhow::Result<(Document, String, crate::abbreviations::LabelAlignment)> {
    use anyhow::Context;
    let result = crate::atom_text::apply(doc, id, text, crate::atom_text::Mode::Auto)
        .map_err(anyhow::Error::msg)?;
    let group = result.abbreviation(id).context("Typed group")?;
    let drawn = group.text(&result).to_owned();
    let alignment = group.alignment;
    Ok((result, drawn, alignment))
}

// Glucose atom IDs: C1 is 1 with its OH oxygen 7; C4 is 4 with the HO oxygen 10.
const C1_OXYGEN: u64 = 7;
const C4_OXYGEN: u64 = 10;

#[test]
fn groups_replacing_the_left_c4_hydroxyl_keep_its_side() -> anyhow::Result<()> {
    use crate::abbreviations::LabelAlignment;
    let doc = sugar_document(Sugar::Glucose, Anomer::Alpha, 42.).map_err(anyhow::Error::msg)?;
    let (ho_left, ho_right) = ink_span(&doc, C4_OXYGEN)?;
    for (typed, drawn) in [("OBn", "BnO"), ("OMe", "MeO"), ("OCH3", "H3CO")] {
        let (result, text, alignment) = typed_group(&doc, C4_OXYGEN, typed)?;
        assert_eq!((text.as_str(), alignment), (drawn, LabelAlignment::Right));
        // O stays at the bond and the rest extends left, as HO did. Glyph
        // widths vary with the platform's fonts, so only the sides are checked.
        let (left, right) = ink_span(&result, C4_OXYGEN)?;
        assert!(
            (right - ho_right).abs() < 0.5,
            "{typed}: {right} vs {ho_right}"
        );
        assert!(left <= ho_left, "{typed}: {left} vs {ho_left}");
        assert!(label_ring_intersections(&result)?.is_empty(), "{typed}");
    }
    Ok(())
}

#[test]
fn a_group_without_a_reverse_spelling_keeps_its_attachment_on_the_c4_bond() -> anyhow::Result<()> {
    use crate::abbreviations::LabelAlignment;
    let doc = sugar_document(Sugar::Glucose, Anomer::Alpha, 42.).map_err(anyhow::Error::msg)?;
    // ONp has no reverse, so forcing Right would put Np, not O, on the bond.
    let (result, text, alignment) = typed_group(&doc, C4_OXYGEN, "ONp")?;
    assert_eq!((text.as_str(), alignment), ("ONp", LabelAlignment::Auto));
    let group = result
        .abbreviation(C4_OXYGEN)
        .ok_or_else(|| anyhow::anyhow!("no group"))?;
    assert_eq!(
        crate::abbreviations::anchor_range(&text, group.faces_left(&result)),
        0..1
    );
    Ok(())
}

#[test]
fn groups_typed_at_the_anomeric_hydroxyl_turn_to_the_free_side() -> anyhow::Result<()> {
    use crate::abbreviations::LabelAlignment;
    let doc = sugar_document(Sugar::Glucose, Anomer::Alpha, 42.).map_err(anyhow::Error::msg)?;
    let (oh_left, oh_right) = ink_span(&doc, C1_OXYGEN)?;
    // Typed MeO is the OMe group, read away from the ring; OMe and SPh are unchanged.
    for (typed, drawn) in [("MeO", "OMe"), ("OMe", "OMe"), ("SPh", "SPh")] {
        let (result, text, alignment) = typed_group(&doc, C1_OXYGEN, typed)?;
        assert_eq!((text.as_str(), alignment), (drawn, LabelAlignment::Auto));
        // The label reads right from the bond, never back over the ring;
        // the first glyph's width varies with the platform's fonts.
        let (left, right) = ink_span(&result, C1_OXYGEN)?;
        assert!(
            left >= oh_left - 2. && right >= oh_right,
            "{typed}: {left} vs {oh_left}"
        );
        assert!(label_ring_intersections(&result)?.is_empty(), "{typed}");
    }
    Ok(())
}

#[test]
fn retyping_a_group_keeps_its_alignment() -> anyhow::Result<()> {
    use crate::abbreviations::LabelAlignment;
    let doc = sugar_document(Sugar::Glucose, Anomer::Alpha, 42.).map_err(anyhow::Error::msg)?;
    for (id, alignment) in [
        (C4_OXYGEN, LabelAlignment::Left),
        (C4_OXYGEN, LabelAlignment::Center),
        (C1_OXYGEN, LabelAlignment::Right),
        (C1_OXYGEN, LabelAlignment::Auto),
        (C1_OXYGEN, LabelAlignment::Above),
    ] {
        let (mut grouped, _, _) = typed_group(&doc, id, "OBn")?;
        grouped
            .abbreviations
            .iter_mut()
            .find(|g| g.anchor == id)
            .ok_or_else(|| anyhow::anyhow!("OBn group"))?
            .alignment = alignment;
        let (_, _, retyped) = typed_group(&grouped, id, "OMe")?;
        assert_eq!(retyped, alignment, "{id}");
    }
    Ok(())
}
