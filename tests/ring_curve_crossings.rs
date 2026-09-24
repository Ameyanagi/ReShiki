use reshiki::{
    document::{Document, Point},
    graphics::{PathCommand, flattened},
    scene::{self, Primitive},
};

const BLUE: [u8; 3] = [32, 80, 145];
fn drawing(tilted: bool, partial: bool, front: bool) -> Result<Document, String> {
    let mut doc = Document::default();
    let metal = doc.add_atom("Fe", Point::default());
    doc.atom_mut(metal).ok_or("metal")?.charge = 2;
    doc = reshiki::hotkeys::atom_edit(&doc, metal, "J", 42.)
        .ok_or("key")??
        .0;
    let ring = doc
        .atoms
        .iter()
        .find(|a| a.attachment.is_some())
        .ok_or("attachment")?
        .centroid
        .clone();
    for bond in &mut doc.bonds {
        if ring.contains(&bond.a) && ring.contains(&bond.b) {
            bond.color = BLUE;
        } else {
            bond.z_order = if front { 10 } else { -10 };
        }
    }
    if partial {
        let selected: Vec<_> = ring
            .iter()
            .filter(|id| doc.atom(**id).is_some_and(|a| a.position.y >= -84.1))
            .copied()
            .collect();
        reshiki::ring_arcs::toggle(&mut doc, &selected)?;
    }
    if tilted {
        reshiki::projection::tilt(&mut doc, &ring, 45., true);
    }
    doc.validate()?;
    Ok(doc)
}
fn curves(doc: &Document) -> Vec<Vec<PathCommand>> {
    scene::primitives(doc)
        .into_iter()
        .filter_map(|p| {
            if let Primitive::Path {
                commands,
                style,
                filled: false,
            } = p
                && style.stroke == BLUE
                && commands.iter().any(|c| matches!(c, PathCommand::Cubic(..)))
            {
                return Some(commands);
            }
            None
        })
        .collect()
}
#[test]
fn front_contacts_clear_circles_and_partial_curves_in_flat_and_tilted_rings() -> Result<(), String>
{
    for tilted in [false, true] {
        for partial in [false, true] {
            for reverse in [false, true] {
                let mut doc = drawing(tilted, partial, true)?;
                if reverse {
                    doc.bonds.last_mut().ok_or("contact")?.reverse();
                }
                let center = doc
                    .atoms
                    .iter()
                    .find(|a| a.attachment.is_some())
                    .ok_or("center")?
                    .position;
                let paths = curves(&doc);
                assert!(!paths.is_empty());
                let points: Vec<_> = paths.iter().flat_map(|p| flattened(p)).flatten().collect();
                let clearance =
                    doc.drawing_style.line_width() * 0.5 + reshiki::style::DEFAULT.world(1.1);
                assert!(
                    points
                        .iter()
                        .filter(|p| p.y > center.y + 0.1)
                        .all(|p| p.x.abs() >= clearance - 0.01),
                    "{tilted}/{partial}/{reverse}"
                );
                if !partial {
                    let mut original = doc.clone();
                    original.bonds.pop();
                    let original_top = curves(&original)
                        .iter()
                        .flat_map(|p| flattened(p))
                        .flatten()
                        .map(|p| p.y)
                        .fold(f32::INFINITY, f32::min);
                    let top = points.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
                    assert!(
                        (top - original_top).abs() < 0.01,
                        "Finite bond must not cut the opposite side"
                    );
                }
                assert!(
                    paths
                        .iter()
                        .map(|p| p
                            .iter()
                            .filter(|c| matches!(c, PathCommand::Cubic(..)))
                            .count())
                        .sum::<usize>()
                        <= 8,
                    "Keep compact cubic geometry"
                );
                let svg = scene::svg(&doc);
                assert!(!svg.contains("rgb(255,255,255)"));
            }
        }
    }
    Ok(())
}
#[test]
fn back_contacts_are_cut_instead_of_the_ring_curve() -> Result<(), String> {
    for tilted in [false, true] {
        for partial in [false, true] {
            let doc = drawing(tilted, partial, false)?;
            let mut uncut = doc.clone();
            uncut.bonds.pop();
            assert_eq!(curves(&doc), curves(&uncut));
            // Adjacent circle/outline gaps may merge at the default ring spacing.
            let segments = scene::primitives(&doc)
                .iter()
                .filter(|p| matches!(p, Primitive::Line(..)))
                .count();
            assert!(segments >= 2, "{tilted}/{partial}: {segments}");
        }
    }
    Ok(())
}

#[test]
fn a_bond_inside_the_ring_is_cut_only_by_the_foreground_circle() -> Result<(), String> {
    let mut doc = drawing(false, false, false)?;
    doc.drawing_style.bond_spacing_ratio = 0.2;
    doc.bonds.pop();
    let center = doc
        .atoms
        .iter()
        .find(|a| a.attachment.is_some())
        .ok_or("center")?
        .position;
    let a = doc.add_atom("C", center.offset(0., -35.));
    let b = doc.add_atom("C", center.offset(0., 35.));
    doc.add_bond(a, b, 1, "plain");
    doc.bonds.last_mut().ok_or("bond")?.z_order = -10;
    assert!(reshiki::crossings::gaps(&doc).iter().all(Vec::is_empty));
    assert_eq!(
        scene::primitives(&doc)
            .iter()
            .filter(|p| matches!(p, Primitive::Line(..)))
            .count(),
        3
    );
    doc.bonds.last_mut().ok_or("bond")?.z_order = 10;
    assert_eq!(
        scene::primitives(&doc)
            .iter()
            .filter(|p| matches!(p, Primitive::Line(..)))
            .count(),
        1
    );
    Ok(())
}

#[test]
fn filled_rings_and_wide_contacts_keep_transparent_clearance() -> Result<(), String> {
    for display in ["plain", "bold", "wedge", "hashed", "dashed"] {
        let mut doc = drawing(true, false, true)?;
        doc.bonds.last_mut().ok_or("contact")?.display = display.into();
        let ring = doc
            .atoms
            .iter()
            .find(|a| a.attachment.is_some())
            .ok_or("ring")?
            .centroid
            .clone();
        assert_eq!(
            reshiki::ring_fills::apply(&mut doc, &ring, Some([245, 221, 165])),
            1
        );
        let stroke = if matches!(display, "bold" | "wedge" | "hashed") {
            doc.drawing_style.world(doc.drawing_style.bold_width_pt)
        } else {
            doc.drawing_style.line_width()
        };
        let center = doc
            .atoms
            .iter()
            .find(|a| a.attachment.is_some())
            .ok_or("center")?
            .position;
        let clearance = stroke / 2. + reshiki::style::DEFAULT.world(1.1);
        assert!(
            curves(&doc)
                .iter()
                .flat_map(|p| flattened(p))
                .flatten()
                .filter(|p| p.y > center.y)
                .all(|p| p.x.abs() >= clearance - 0.01),
            "{display}"
        );
        let svg = scene::svg(&doc);
        assert!(!svg.contains("rgb(255,255,255)"));
        assert!(scene::primitives(&doc).iter().any(|p| matches!(p, Primitive::Path { filled: true, style, .. } if style.fill == Some([245, 221, 165]))));
        let restored: Document =
            serde_json::from_str(&serde_json::to_string(&doc).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        assert_eq!(curves(&doc), curves(&restored));
    }
    Ok(())
}

#[test]
fn non_crossing_contact_preserves_the_whole_circle() -> Result<(), String> {
    let mut doc = drawing(false, false, true)?;
    let contact = doc.bonds.last().ok_or("contact")?.clone();
    let center = doc.atom(contact.b).ok_or("attachment")?.position;
    // A contact wholly inside the inner circle has no crossing to clear.
    doc.atom_mut(contact.a).ok_or("metal")?.position = center.offset(0., 10.);
    let mut without_contact = doc.clone();
    without_contact.bonds.pop();
    assert_eq!(curves(&doc), curves(&without_contact));
    Ok(())
}
