use crate::{
    bond_joins,
    bonds::{BondPreset, DoublePosition},
    document::{Document, Point},
    editing,
    scene::{self, Primitive},
};

fn chain() -> Document {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(0., 21.));
    let b = doc.add_atom("C", Point::new(36.373, 0.));
    let c = doc.add_atom("C", Point::new(72.746, 21.));
    let d = doc.add_atom("C", Point::new(109.119, 0.));
    doc.add_bond(a, b, 1, "plain");
    BondPreset::BoldDouble.place(&mut doc, b, c);
    doc.add_bond(c, d, 1, "plain");
    doc
}

fn raster(doc: &Document) -> Result<(resvg::tiny_skia::Pixmap, Point, f32), String> {
    let svg = scene::svg(doc);
    let xml = roxmltree::Document::parse(&svg).map_err(|e| e.to_string())?;
    let bounds: Vec<f32> = xml
        .root_element()
        .attribute("viewBox")
        .ok_or("Missing viewBox")?
        .split_whitespace()
        .map(|s| {
            s.parse()
                .map_err(|e: std::num::ParseFloatError| e.to_string())
        })
        .collect::<Result<_, _>>()?;
    let [x, y, width, _] = bounds.as_slice() else {
        return Err("Invalid viewBox".into());
    };
    let tree = resvg::usvg::Tree::from_str(
        &svg,
        &resvg::usvg::Options {
            dpi: 72.,
            ..Default::default()
        },
    )
    .map_err(|e| e.to_string())?;
    let scale = 12.;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(
        (tree.size().width() * scale).ceil() as u32,
        (tree.size().height() * scale).ceil() as u32,
    )
    .ok_or("Cannot allocate raster")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Ok((
        pixmap,
        Point::new(*x, *y),
        tree.size().width() * scale / width,
    ))
}

#[test]
fn colored_branches_join_the_same_ring_outline_without_changing_chemistry() -> anyhow::Result<()> {
    use anyhow::Context;
    let source: Document = serde_json::from_str(include_str!(
        "../../docs/changes/fixtures/arene-bold-join.rsk"
    ))?;
    for rotation in [0., 47., 130.] {
        for reversed in [false, true] {
            for color in [[43, 112, 97], [0, 0, 0], [180, 68, 32]] {
                let mut doc = source.clone();
                let fluorine = doc
                    .atoms
                    .iter()
                    .find(|a| a.element == "F")
                    .context("Fluorine")?
                    .id;
                let branch = doc
                    .bonds
                    .iter_mut()
                    .find(|b| b.a == fluorine || b.b == fluorine)
                    .context("Branch")?;
                branch.color = color;
                let carbon = if branch.a == fluorine {
                    branch.b
                } else {
                    branch.a
                };
                if reversed {
                    for bond in &mut doc.bonds {
                        bond.reverse();
                    }
                    doc.bonds.reverse();
                }
                let ids = doc.all_ids();
                editing::transform_about(&mut doc, &ids, Point::default(), 1., rotation);
                let saved = doc.clone();
                let (pixmap, origin, scale) = raster(&doc).map_err(anyhow::Error::msg)?;
                let point = doc.atom(carbon).context("Carbon")?.position;
                for bond in doc.bonds.iter().filter(|b| b.a == carbon || b.b == carbon) {
                    let end = doc
                        .atom(if bond.a == carbon { bond.b } else { bond.a })
                        .context("Neighbor")?
                        .position;
                    for t in [0., 0.01, 0.03, 0.08, 0.15] {
                        let sample = Point::new(
                            point.x + (end.x - point.x) * t,
                            point.y + (end.y - point.y) * t,
                        );
                        let pixel = pixmap
                            .pixel(
                                ((sample.x - origin.x) * scale).floor() as u32,
                                ((sample.y - origin.y) * scale).floor() as u32,
                            )
                            .context("Pixel")?;
                        anyhow::ensure!(
                            pixel.alpha() >= 250,
                            "Junction gap: {rotation}, {reversed}, {color:?}, {t}: {}",
                            pixel.alpha()
                        );
                    }
                }
                assert_eq!(doc, saved);
                let mut uniform = doc.clone();
                for bond in &mut uniform.bonds {
                    bond.color = [0, 0, 0];
                }
                for (colored, plain) in doc.bonds.iter().zip(&uniform.bonds) {
                    let start = doc.atom(colored.a).context("Start")?.position;
                    let end = doc.atom(colored.b).context("End")?.position;
                    assert_eq!(
                        bond_joins::polygon(&doc, colored, start, end),
                        bond_joins::polygon(&uniform, plain, start, end),
                        "Color must not change junction geometry"
                    );
                }
            }
        }
    }
    Ok(())
}

#[test]
fn a_substituent_does_not_deform_the_thick_ring_corner() -> anyhow::Result<()> {
    use anyhow::Context;
    let source: Document = serde_json::from_str(include_str!(
        "../../docs/changes/fixtures/arene-bold-join.rsk"
    ))?;
    for tilt in [-25., 0., 20.] {
        for rotation in [0., 47., 130.] {
            let mut doc = source.clone();
            let ids = doc.all_ids();
            crate::projection::tilt(&mut doc, &ids, tilt, true);
            // Keep this regression's thick/thin junction at the substituted
            // carbon; automatic depth emphasis may move to another ring edge.
            for (bond, original) in doc.bonds.iter_mut().zip(&source.bonds) {
                bond.display.clone_from(&original.display);
            }
            editing::transform_about(&mut doc, &ids, Point::default(), 1., rotation);
            let mut bare = doc.clone();
            let fluorine = bare
                .atoms
                .iter()
                .find(|a| a.element == "F")
                .context("F")?
                .id;
            bare.bonds.retain(|b| b.a != fluorine && b.b != fluorine);
            bare.atoms.retain(|a| a.id != fluorine);
            for bond in &bare.bonds {
                let attached = doc
                    .bonds
                    .iter()
                    .find(|b| b.a == bond.a && b.b == bond.b)
                    .context("Ring edge")?;
                let a = bare.atom(bond.a).context("A")?.position;
                let b = bare.atom(bond.b).context("B")?.position;
                for (p, q) in bond_joins::polygon(&doc, attached, a, b)
                    .iter()
                    .zip(bond_joins::polygon(&bare, bond, a, b))
                {
                    anyhow::ensure!(
                        p.distance(q) < 0.001,
                        "Substituent changes ring silhouette: tilt {tilt}, rotation {rotation}, edge {bond:?}, corners {p:?}/{q:?}"
                    );
                }
            }
            // Inside the ring's ink the branch must not introduce its color.
            let branch = doc
                .bonds
                .iter()
                .find(|b| b.a == fluorine || b.b == fluorine)
                .context("Branch")?;
            let carbon = if branch.a == fluorine {
                branch.b
            } else {
                branch.a
            };
            let (pixmap, origin, scale) = raster(&doc).map_err(anyhow::Error::msg)?;
            let point = doc.atom(carbon).context("Carbon")?.position;
            let pixel = pixmap
                .pixel(
                    ((point.x - origin.x) * scale).floor() as u32,
                    ((point.y - origin.y) * scale).floor() as u32,
                )
                .context("Pixel")?;
            anyhow::ensure!(
                pixel.alpha() == 255 && pixel.red() == 0 && pixel.green() == 0 && pixel.blue() == 0,
                "Colored branch intrudes into the ring"
            );
        }
    }
    Ok(())
}

#[test]
fn automatic_ring_double_lines_share_corners_with_a_bold_single_edge() -> Result<(), String> {
    for angle in [0., 25., 55., 75.] {
        for reversed in [false, true] {
            let mut doc = crate::rings::Preset::Benzene.document(42., false);
            let ids = doc.all_ids();
            crate::projection::tilt(&mut doc, &ids, angle, false);
            let bold = doc
                .bonds
                .iter_mut()
                .find(|b| b.order == 1)
                .ok_or("Single edge")?;
            bold.display = "bold".into();
            if reversed {
                bold.reverse();
            }
            let bold = doc
                .bonds
                .iter()
                .find(|b| b.display == "bold")
                .ok_or("Bold edge")?;
            let polygon = |b: &crate::document::Bond| -> Result<Vec<Point>, String> {
                Ok(bond_joins::polygon(
                    &doc,
                    b,
                    doc.atom(b.a).ok_or("Atom")?.position,
                    doc.atom(b.b).ok_or("Atom")?.position,
                ))
            };
            let corners = polygon(bold)?;
            for double in doc.bonds.iter().filter(|b| {
                b.order == 2 && [b.a, b.b].iter().any(|id| *id == bold.a || *id == bold.b)
            }) {
                assert_eq!(double.double_position, DoublePosition::Auto);
                assert!(bond_joins::needed(&doc, double));
                let adjacent = polygon(double)?;
                assert_eq!(
                    corners
                        .iter()
                        .filter(|p| adjacent.iter().any(|q| p.distance(*q) < 0.001))
                        .count(),
                    2,
                    "Unjoined bold cap: tilt {angle}, reversed {reversed}"
                );
            }
        }
    }
    Ok(())
}
#[test]
fn automatic_bold_double_has_a_continuous_backbone_and_a_separate_thin_rail() -> Result<(), String>
{
    for rotation in [0., 17., 83., 145.] {
        for reversed in [false, true] {
            for (thin, bold) in [(0.4, 1.5), (0.6, 2.), (1.2, 2.5)] {
                for color in [[0, 0, 0], [180, 68, 32]] {
                    let mut doc = chain();
                    doc.drawing_style.line_width_pt = thin;
                    doc.drawing_style.bold_width_pt = bold;
                    for bond in &mut doc.bonds {
                        bond.color = color;
                    }
                    if reversed {
                        doc.bonds.get_mut(1).ok_or("Missing double")?.reverse();
                    }
                    let ids = doc.all_ids();
                    editing::transform_about(&mut doc, &ids, Point::default(), 1., rotation);
                    let original = doc.clone();
                    let double = doc.bonds.get(1).ok_or("Missing double")?;
                    assert_ne!(
                        scene::effective_double_position(&doc, double),
                        DoublePosition::Center
                    );
                    let (pixmap, origin, scale) = raster(&doc)?;
                    let pixel = |p: Point| {
                        pixmap
                            .pixel(
                                ((p.x - origin.x) * scale).floor() as u32,
                                ((p.y - origin.y) * scale).floor() as u32,
                            )
                            .ok_or("Sample outside image")
                    };
                    // Sample the actual skeleton through both junctions, not just
                    // the center of each stroke where even detached rails pass.
                    for bond in &doc.bonds {
                        let a = doc.atom(bond.a).ok_or("Missing atom")?.position;
                        let b = doc.atom(bond.b).ok_or("Missing atom")?.position;
                        for t in [0., 0.01, 0.03, 0.1, 0.5, 0.9, 0.97, 0.99, 1.] {
                            if (t == 0. && bond.a == 1) || (t == 1. && bond.b == 4) {
                                continue;
                            }
                            let p = Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
                            assert_eq!(
                                pixel(p)?.alpha(),
                                255,
                                "Disconnected skeleton: rotation {rotation}, reversed {reversed}, widths {thin}/{bold}, t {t}"
                            );
                        }
                    }
                    let a = doc.atom(double.a).ok_or("Missing atom")?.position;
                    let b = doc.atom(double.b).ok_or("Missing atom")?.position;
                    let mid = Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.);
                    let n = Point::new(-(b.y - a.y) / a.distance(b), (b.x - a.x) / a.distance(b));
                    let spacing =
                        doc.drawing_style.bond_length_world * doc.drawing_style.bond_spacing_ratio;
                    let rail = mid.offset(n.x * spacing, n.y * spacing);
                    assert_eq!(pixel(rail)?.alpha(), 255, "Secondary rail was dropped");
                    let gap = (doc.drawing_style.world(bold) / 2. + spacing
                        - doc.drawing_style.line_width() / 2.)
                        / 2.;
                    assert_eq!(
                        pixel(mid.offset(n.x * gap, n.y * gap))?.alpha(),
                        0,
                        "Double bond became a filled bar"
                    );
                    assert_eq!(doc, original, "Rendering cannot change chemical state");
                }
            }
        }
    }
    Ok(())
}

#[test]
fn offset_double_backbones_share_both_corner_coordinates_with_single_and_wedge_neighbors()
-> Result<(), String> {
    for preset in [
        BondPreset::Double,
        BondPreset::BoldDouble,
        BondPreset::DashedDouble,
    ] {
        for position in [DoublePosition::Left, DoublePosition::Right] {
            for neighbor in [BondPreset::Single, BondPreset::Bold, BondPreset::Wedge] {
                let mut doc = chain();
                for b in &mut doc.bonds {
                    neighbor.apply(b);
                }
                let middle = doc.bonds.get_mut(1).ok_or("Missing double")?;
                preset.apply(middle);
                middle.double_position = position;
                let middle = doc.bonds.get(1).ok_or("Missing double")?;
                let polygon = |bond: &crate::document::Bond| -> Result<Vec<Point>, String> {
                    Ok(bond_joins::polygon(
                        &doc,
                        bond,
                        doc.atom(bond.a).ok_or("Missing atom")?.position,
                        doc.atom(bond.b).ok_or("Missing atom")?.position,
                    ))
                };
                let main = polygon(middle)?;
                assert!(bond_joins::needed(&doc, middle));
                for adjacent in doc.bonds.iter().filter(|b| b.order == 1) {
                    let adjacent = polygon(adjacent)?;
                    assert_eq!(
                        main.iter()
                            .filter(|p| adjacent.iter().any(|q| p.distance(*q) < 0.001))
                            .count(),
                        2
                    );
                }
                let svg = scene::svg(&doc);
                if preset == BondPreset::DashedDouble {
                    assert_eq!(
                        svg.matches("stroke-dasharray").count(),
                        1,
                        "Dashed secondary rail lost"
                    );
                }
            }
        }
    }
    Ok(())
}

#[test]
fn centered_bold_rail_has_flat_caps_and_keeps_the_requested_placement() -> Result<(), String> {
    let mut doc = chain();
    doc.bonds
        .get_mut(1)
        .ok_or("Missing double")?
        .double_position = DoublePosition::Center;
    assert_eq!(
        scene::effective_double_position(&doc, doc.bonds.get(1).ok_or("Missing double")?),
        DoublePosition::Center
    );
    let bold_width = doc.drawing_style.world(doc.drawing_style.bold_width_pt);
    let primitives = scene::primitives(&doc);
    assert!(
        primitives
            .iter()
            .any(|p| matches!(p, Primitive::Polygon(points) if points.len() == 4))
    );
    assert!(
        !primitives
            .iter()
            .any(|p| matches!(p, Primitive::Line(_, _, width) if *width == bold_width)),
        "Round-capped bold stroke protrudes beyond the endpoints"
    );
    Ok(())
}

#[test]
fn alternating_ring_outline_stays_opaque_when_double_edges_are_bold_or_tilted() -> Result<(), String>
{
    for bold in [false, true] {
        for tilt in [0., 45., 70.] {
            let mut doc = Document::default();
            let ids = editing::ring(&mut doc, Point::default(), 6, false, 0.);
            for (i, bond) in doc.bonds.iter_mut().enumerate() {
                if i % 2 == 0 {
                    (if bold {
                        BondPreset::BoldDouble
                    } else {
                        BondPreset::Double
                    })
                    .apply(bond);
                }
            }
            crate::projection::tilt(&mut doc, &ids, tilt, true);
            let (pixmap, origin, scale) = raster(&doc)?;
            for bond in &doc.bonds {
                let a = doc.atom(bond.a).ok_or("Missing atom")?.position;
                let b = doc.atom(bond.b).ok_or("Missing atom")?.position;
                for t in [0., 0.02, 0.1, 0.5, 0.9, 0.98, 1.] {
                    let p = Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
                    let pixel = pixmap
                        .pixel(
                            ((p.x - origin.x) * scale).floor() as u32,
                            ((p.y - origin.y) * scale).floor() as u32,
                        )
                        .ok_or("Outside raster")?;
                    assert_eq!(
                        pixel.alpha(),
                        255,
                        "Ring gap: bold {bold}, tilt {tilt}, t {t}"
                    );
                }
            }
        }
    }
    Ok(())
}
