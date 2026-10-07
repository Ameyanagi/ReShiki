use super::*;

#[test]
fn ring_hotkeys_bisect_the_incoming_bond_at_any_chain_rotation() -> Result<(), String> {
    for length in [24., 42., 73.] {
        for rotation in [0_f32, 7.3, 30., 67.5, 138., 223., 283., 345.] {
            for mirrored in [-1., 1.] {
                let mut chain = Document::default();
                let mut ids = Vec::new();
                let mut point = Point::new(170., -80.);
                for index in 0..4 {
                    ids.push(chain.add_atom("C", point));
                    let theta = (rotation + mirrored * if index % 2 == 0 { -30. } else { 30. })
                        .to_radians();
                    point = point.offset(length * theta.cos(), length * theta.sin());
                }
                for (index, pair) in ids.windows(2).enumerate() {
                    if let [a, b] = pair {
                        chain.add_bond(*a, *b, if index == 0 { 2 } else { 1 }, "plain");
                    }
                }
                let anchor = *ids.last().ok_or("Missing terminal atom")?;
                let terminal = chain.atom(anchor).ok_or("Missing atom")?.position;
                let neighbor = chain
                    .atoms
                    .iter()
                    .rev()
                    .nth(1)
                    .ok_or("Missing neighbor")?
                    .position;
                let incoming = Point::new(neighbor.x - terminal.x, neighbor.y - terminal.y);
                for (key, size) in [("3", 6), ("a", 6), ("6", 6), ("7", 5), ("v", 3), ("u", 4)] {
                    let (placed, ring) = ring_edit(&chain, Some(anchor), None, key, length)
                        .ok_or("Missing ring key")??;
                    for atom in &chain.atoms {
                        assert_eq!(
                            placed.atom(atom.id).ok_or("Lost chain atom")?.position,
                            atom.position
                        );
                    }
                    let junctions: Vec<_> = placed
                        .bonds
                        .iter()
                        .filter_map(|b| {
                            let other = if b.a == anchor {
                                b.b
                            } else if b.b == anchor {
                                b.a
                            } else {
                                return None;
                            };
                            ring.contains(&other).then_some(other)
                        })
                        .collect();
                    assert_eq!(junctions.len(), 2);
                    let expected_angle =
                        std::f32::consts::FRAC_PI_2 + std::f32::consts::PI / size as f32;
                    for id in junctions {
                        let p = placed.atom(id).ok_or("Missing ring neighbor")?.position;
                        let outgoing = Point::new(p.x - terminal.x, p.y - terminal.y);
                        let cosine = (incoming.x * outgoing.x + incoming.y * outgoing.y)
                            / (incoming.distance(Point::default())
                                * outgoing.distance(Point::default()));
                        assert!(
                            (cosine - expected_angle.cos()).abs() < 0.0001,
                            "{key}, rotation {rotation}, length {length}, mirror {mirrored}: expected junction {expected_angle} rad, cosine {cosine}"
                        );
                    }
                    for bond in placed
                        .bonds
                        .iter()
                        .filter(|b| ring.contains(&b.a) && ring.contains(&b.b))
                    {
                        let a = placed.atom(bond.a).ok_or("Missing bond end")?.position;
                        let b = placed.atom(bond.b).ok_or("Missing bond end")?.position;
                        assert!((a.distance(b) - length).abs() < 0.001);
                    }
                    placed.validate()?;
                }
            }
        }
    }
    Ok(())
}
