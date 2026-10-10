use super::*;
use crate::{depth_appearance as depth, projection, scene};

fn cage(source: &str) -> Document {
    let mut doc = Document::from_json(source.as_bytes()).unwrap();
    let ids = doc.all_ids();
    depth::set_rear_opacity(&mut doc, &ids, 0.25).unwrap();
    doc
}
fn cross(a: Point, b: Point, c: Point) -> f64 {
    (f64::from(b.x) - f64::from(a.x)) * (f64::from(c.y) - f64::from(a.y))
        - (f64::from(b.y) - f64::from(a.y)) * (f64::from(c.x) - f64::from(a.x))
}
fn rim(doc: &Document) -> Vec<u64> {
    let mut points: Vec<_> = doc.atoms.iter().map(|a| (a.position, a.id)).collect();
    points.sort_by(|a, b| a.0.x.total_cmp(&b.0.x).then(a.0.y.total_cmp(&b.0.y)));
    let mut low: Vec<(Point, u64)> = vec![];
    let mut high: Vec<(Point, u64)> = vec![];
    for &p in &points {
        while low.len() >= 2 && cross(low[low.len() - 2].0, low[low.len() - 1].0, p.0) <= 0. {
            low.pop();
        }
        low.push(p);
    }
    for &p in points.iter().rev() {
        while high.len() >= 2 && cross(high[high.len() - 2].0, high[high.len() - 1].0, p.0) <= 0. {
            high.pop();
        }
        high.push(p);
    }
    low.pop();
    high.pop();
    low.extend(high);
    low.into_iter().map(|(_, id)| id).collect()
}
fn sources() -> [&'static str; 2] {
    [
        include_str!("../../../../tests/fixtures/rear-opacity/c60-rear-opacity-25.rsk"),
        include_str!("../../../../tests/fixtures/projected-double-bonds/c70.rsk"),
    ]
}
fn rotate(doc: &mut Document, degrees: f64) {
    let n = doc.atoms.len() as f64;
    let center = doc.atoms.iter().fold([0_f64; 3], |mut c, a| {
        c[0] += f64::from(a.position.x) / n;
        c[1] += f64::from(a.position.y) / n;
        c[2] += f64::from(a.depth) / n;
        c
    });
    let (sin, cos) = degrees.to_radians().sin_cos();
    for a in &mut doc.atoms {
        let y = f64::from(a.position.y) - center[1];
        let z = f64::from(a.depth) - center[2];
        a.position.y = (center[1] + y * cos - z * sin) as f32;
        a.depth = (center[2] + y * sin + z * cos) as f32;
    }
}
#[test]
fn view_visibility_cages_keep_entire_rim_and_fade_hidden_interior() {
    for source in sources() {
        let mut doc = cage(source);
        let original = doc.clone();
        let hull = rim(&doc);
        assert_eq!(hull.len(), 20);
        for alpha in [0., 0.25] {
            let ids = doc.all_ids();
            depth::set_rear_opacity(&mut doc, &ids, alpha).unwrap();
            let paint = Paint::new(&doc);
            for id in &hull {
                assert_eq!(paint.atom(*id), 1., "Exposed rim atom {id} faded");
            }
            let mut tested_edges = 0;
            for (&a, &b) in hull
                .iter()
                .zip(hull.iter().cycle().skip(1))
                .take(hull.len())
            {
                let bond = doc
                    .bonds
                    .iter()
                    .find(|bond| [bond.a, bond.b].contains(&a) && [bond.a, bond.b].contains(&b));
                let Some(bond) = bond else {
                    continue;
                };
                tested_edges += 1;
                for t in [0., 0.1, 0.25, 0.5, 0.75, 0.9, 1.] {
                    assert_eq!(paint.bond(bond, t), 1., "Rim bond {a}-{b} faded at {t}");
                }
                let x = doc.atom(a).unwrap().position;
                let y = doc.atom(b).unwrap().position;
                let width = doc.drawing_style.line_width() * 3.;
                let parts = paint.bond_parts(&doc, bond, vec![Primitive::Line(x, y, width)]);
                assert!(
                    matches!(parts.as_slice(),[Primitive::Line(first,last,w)] if *first==x&&*last==y&&*w==width),
                    "Complete rim stroke changed"
                );
            }
            assert!(
                tested_edges >= 10,
                "No meaningful exposed graph edges tested"
            );
            if doc.atoms.len() == 60 {
                assert_eq!(tested_edges, 20);
            }
            assert!(
                doc.atoms.iter().any(|a| paint.atom(a.id) == alpha),
                "Hidden cage atoms stayed solid"
            );
            assert!(
                doc.bonds.iter().any(|b| paint.bond(b, 0.5) == alpha),
                "Hidden cage bonds stayed solid"
            );
        }
        assert_eq!(doc.atoms, original.atoms);
        assert_eq!(doc.bonds, original.bonds);
        assert!(!crate::transaction::chemistry_changed(&original, &doc));
    }
}
#[test]
fn view_visibility_rotated_rim_first_failure_diagnostic() {
    let mut doc = cage(include_str!(
        "../../../../tests/fixtures/projected-double-bonds/c60.rsk"
    ));
    rotate(&mut doc, 17.);
    let atom = doc.atom(41).unwrap();
    let visibility = visibility::Visibility::new(&doc);
    let (triangles, ink) = visibility.raw_atom_hits(atom.position, atom.depth, &[41]);
    println!(
        "Raw 17-degree atom41: triangle faces={triangles:?}, ink owners={ink:?}, rim={}, finalalpha={}",
        rim(&doc).contains(&41),
        Paint::new(&doc).atom(41)
    );
    assert!(triangles.is_empty());
    assert!(ink.iter().any(|owners| owners.as_slice() == [17, 27]));
    assert!(rim(&doc).contains(&41));
    assert_eq!(Paint::new(&doc).atom(41), 1.);
}
#[test]
fn view_visibility_rotations_are_live_and_opaque_output_is_exact() {
    for source in sources() {
        let original = cage(source);
        let ids = original.all_ids();
        for angle in [0., 17., 89.999, 90., 90.001, 127., 180., 270.] {
            let mut doc = original.clone();
            rotate(&mut doc, angle);
            assert_eq!(
                visibility::Visibility::new(&doc).face_count(),
                if doc.atoms.len() == 60 { 32 } else { 37 },
                "Rotated cage was rejected at {angle}"
            );
            let hull = rim(&doc);
            for alpha in [0., 0.25] {
                depth::set_rear_opacity(&mut doc, &ids, alpha).unwrap();
                let paint = Paint::new(&doc);
                for id in &hull {
                    assert_eq!(paint.atom(*id), 1., "Rotated rim atom {id}/{angle}/{alpha}");
                }
                let mut exposed_edges = 0;
                for b in &doc.bonds {
                    let a = doc.atom(b.a).unwrap().position;
                    let z = doc.atom(b.b).unwrap().position;
                    let on = |p: Point, x: Point, y: Point| {
                        cross(x, y, p).abs() <= 1e-4 * f64::from(x.distance(y))
                            && p.x >= x.x.min(y.x) - 1e-4
                            && p.x <= x.x.max(y.x) + 1e-4
                            && p.y >= x.y.min(y.y) - 1e-4
                            && p.y <= x.y.max(y.y) + 1e-4
                    };
                    if hull
                        .iter()
                        .zip(hull.iter().cycle().skip(1))
                        .take(hull.len())
                        .any(|(x, y)| {
                            let x = doc.atom(*x).unwrap().position;
                            let y = doc.atom(*y).unwrap().position;
                            on(a, x, y) && on(z, x, y)
                        })
                    {
                        exposed_edges += 1;
                        for t in [0., 0.1, 0.25, 0.5, 0.75, 0.9, 1.] {
                            assert_eq!(
                                paint.bond(b, t),
                                1.,
                                "Rotated rim bond {}-{} at {angle}/{alpha}/{t}",
                                b.a,
                                b.b
                            );
                        }
                        let width = doc.drawing_style.line_width() * 3.;
                        let parts = paint.bond_parts(&doc, b, vec![Primitive::Line(a, z, width)]);
                        assert!(
                            matches!(parts.as_slice(),[Primitive::Line(first,last,w)] if *first==a&&*last==z&&*w==width),
                            "Rotated full stroke at {angle}/{alpha}"
                        );
                    }
                    for t in [0., 0.5, 1.] {
                        assert!(paint.bond(b, t).is_finite());
                    }
                }
                assert!(
                    exposed_edges > 0,
                    "No real exposed graph edge checked at {angle}"
                );
                assert!(
                    doc.atoms.iter().any(|a| paint.atom(a.id) == alpha),
                    "No rear cage ink changed at {angle}/{alpha}"
                );
            }
            let geometry = doc.clone();
            let mut clear = doc.clone();
            clear.depth_appearance.clear();
            let svg = scene::svg(&clear);
            depth::set_rear_opacity(&mut doc, &ids, 1.).unwrap();
            assert_eq!(scene::svg(&doc), svg);
            assert_eq!(doc.atoms, geometry.atoms);
            assert_eq!(doc.bonds, geometry.bonds);
        }
        let before = Paint::new(&original);
        let mut turned = original.clone();
        rotate(&mut turned, 180.);
        let after = Paint::new(&turned);
        assert!(
            ids.iter().any(|id| before.atom(*id) != after.atom(*id)),
            "Rotation did not recompute front/rear visibility"
        );
    }
}
#[test]
fn view_visibility_open_chains_and_single_tilted_rings_do_not_invent_surfaces() {
    let mut chain = Document::default();
    let ids: Vec<_> = [-30., -10., 10., 30.]
        .into_iter()
        .enumerate()
        .map(|(i, z)| {
            let id = chain.add_atom("C", Point::new(i as f32 * 42., 0.));
            chain.atom_mut(id).unwrap().depth = z;
            id
        })
        .collect();
    for pair in ids.windows(2) {
        chain.add_bond(pair[0], pair[1], 1, "plain");
    }
    let plain = scene::svg(&chain);
    depth::set_rear_opacity(&mut chain, &ids, 0.).unwrap();
    assert_eq!(scene::svg(&chain), plain);
    let mut ring = Document::default();
    let ids = crate::editing::ring(&mut ring, Point::default(), 6, false, 0.);
    projection::tilt(&mut ring, &ids, 50., true);
    let svg = scene::svg(&ring);
    depth::set_rear_opacity(&mut ring, &ids, 0.).unwrap();
    assert_eq!(scene::svg(&ring), svg);
}
#[test]
fn view_visibility_hidden_middle_is_split_without_fading_exposed_endpoints() {
    let mut doc = Document::default();
    let ids: Vec<_> = [
        (-80., 0., -20.),
        (80., 0., -20.),
        (0., -50., 20.),
        (0., 50., 20.),
    ]
    .into_iter()
    .map(|(x, y, z)| {
        let id = doc.add_atom("C", Point::new(x, y));
        doc.atom_mut(id).unwrap().depth = z;
        id
    })
    .collect();
    doc.add_bond(ids[0], ids[1], 1, "plain");
    doc.add_bond(ids[2], ids[3], 1, "plain");
    doc.add_bond(ids[0], ids[2], 1, "plain");
    depth::set_rear_opacity(&mut doc, &ids, 0.25).unwrap();
    let paint = Paint::new(&doc);
    let bond = &doc.bonds[0];
    assert_eq!(paint.bond(bond, 0.), 1.);
    assert_eq!(paint.bond(bond, 1.), 1.);
    assert_eq!(paint.bond(bond, 0.5), 0.25);
    let parts = paint.bond_parts(
        &doc,
        bond,
        vec![Primitive::Line(
            doc.atom(bond.a).unwrap().position,
            doc.atom(bond.b).unwrap().position,
            2.,
        )],
    );
    assert!(
        parts
            .iter()
            .any(|p| matches!(p,Primitive::Opacity{alpha,..} if *alpha==0.25))
    );
    assert!(
        parts
            .iter()
            .filter(|p| !matches!(p, Primitive::Opacity { .. }))
            .count()
            >= 2
    );
    assert!(
        crate::crossings::gaps(&doc)[0].is_empty(),
        "Conventional knockout erased managed faint ink"
    );
}
#[test]
fn view_visibility_freezing_rgb_keeps_settings_but_visibility_tracks_rotation() {
    let mut doc = cage(sources()[0]);
    let ids = doc.all_ids();
    depth::enable(&mut doc, &ids, 0.6).unwrap();
    depth::freeze(&mut doc, &ids);
    let settings = doc.depth_appearance.clone();
    let before = Paint::new(&doc);
    rotate(&mut doc, 180.);
    assert_eq!(doc.depth_appearance, settings);
    let after = Paint::new(&doc);
    assert!(ids.iter().any(|id| before.atom(*id) != after.atom(*id)));
    let reopened = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(reopened.version, crate::document::VERSION);
    assert_eq!(reopened, doc.current());
}

#[test]
fn view_visibility_validates_complete_mesh_and_rejects_partial_folded_or_duplicate_cages() {
    for source in sources() {
        let original = cage(source);
        assert_eq!(
            visibility::Visibility::new(&original).face_count(),
            if original.atoms.len() == 60 { 32 } else { 37 }
        );
        for fault in 0..4 {
            let mut doc = original.clone();
            match fault {
                0 => {
                    doc.bonds.pop();
                }
                1 => {
                    let position = doc.atoms[1].position;
                    let depth = doc.atoms[1].depth;
                    doc.atoms[0].position = position;
                    doc.atoms[0].depth = depth;
                }
                2 => {
                    doc.atoms[0].position = Point::default();
                    doc.atoms[0].depth = 0.;
                }
                _ => doc.bonds.push(doc.bonds[0].clone()),
            }
            assert_eq!(
                visibility::Visibility::new(&doc).face_count(),
                0,
                "Fault {fault} inferred a surface"
            );
        }
        let mut reordered = original.clone();
        reordered.atoms.reverse();
        reordered.bonds.reverse();
        for b in &mut reordered.bonds {
            std::mem::swap(&mut b.a, &mut b.b);
        }
        assert_eq!(
            visibility::Visibility::new(&reordered).face_count(),
            visibility::Visibility::new(&original).face_count()
        );
        let a = Paint::new(&original);
        let b = Paint::new(&reordered);
        for atom in &original.atoms {
            assert_eq!(a.atom(atom.id), b.atom(atom.id));
        }
        for bond in &original.bonds {
            let reverse = reordered
                .bonds
                .iter()
                .find(|b| b.a == bond.b && b.b == bond.a)
                .unwrap();
            for t in [0., 0.25, 0.5, 0.75, 1.] {
                assert_eq!(a.bond(bond, t), b.bond(reverse, 1. - t));
            }
        }
    }
}
#[test]
fn view_visibility_query_budget_tracks_nonopaque_owners_and_independent_cages() {
    let mut doc = cage(sources()[0]);
    doc.depth_appearance.clear();
    let first = doc.all_ids();
    let mut other = doc.clone();
    for atom in &mut other.atoms {
        atom.depth += 130.;
    }
    let second = crate::editing::append(&mut doc, &other, Point::new(400., 0.));
    doc.add_bond(first[0], second[0], 5, "plain");
    let all = doc.all_ids();
    depth::enable(&mut doc, &all, 0.6).unwrap();
    assert_eq!(doc.depth_appearance.len(), 1, "legacy coordinate RGB owner");
    let original = doc.clone();
    let original_svg = scene::svg(&doc);
    let rgb = depth::Paint::new(&doc);
    depth::set_rear_opacity(&mut doc, &[first[0]], 0.25).unwrap();
    let first_edit = doc.clone();
    let paint = Paint::new(&doc);
    assert!(!paint.is_empty(), "An opaque neighbor disabled cage fading");
    assert!(first.iter().any(|id| paint.atom(*id) == 0.25));
    assert!(
        doc.bonds
            .iter()
            .filter(|b| first.contains(&b.a) && first.contains(&b.b))
            .any(|b| paint.bond(b, 0.5) == 0.25)
    );
    assert!(second.iter().all(|id| paint.atom(*id) == 1.));
    assert!(
        doc.bonds
            .iter()
            .filter(|b| second.contains(&b.a) && second.contains(&b.b))
            .all(|b| paint.bond(b, 0.5) == 1.)
    );
    assert_eq!(depth::rear_opacity(&doc, &second), Some(1.));
    let mut history = crate::document::History::default();
    assert!(history.commit(original.clone(), &doc));
    assert!(history.undo(&mut doc));
    assert_eq!(doc, original);
    assert!(history.redo(&mut doc));
    assert_eq!(doc, first_edit);
    depth::set_rear_opacity(&mut doc, &[second[0]], 0.5).unwrap();
    let paint = Paint::new(&doc);
    assert!(
        !paint.is_empty(),
        "Two ordinary cages exceeded the query estimate"
    );
    assert!(first.iter().any(|id| paint.atom(*id) == 0.25));
    assert!(second.iter().any(|id| paint.atom(*id) == 0.5));
    let reopened = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(reopened.version, crate::document::VERSION);
    assert_eq!(reopened, doc.current());
    for id in &all {
        assert_eq!(depth::Paint::new(&doc).amount(*id), rgb.amount(*id));
    }
    assert_eq!(
        doc.depth_appearance[0].atoms,
        original.depth_appearance[0].atoms
    );
    assert_eq!(
        doc.depth_appearance[0].weights,
        original.depth_appearance[0].weights
    );
    let mut nonpaint = doc.clone();
    nonpaint.version = original.version;
    nonpaint.depth_appearance = original.depth_appearance.clone();
    assert_eq!(nonpaint, original, "All native graph, XYZ and other fields");
    assert!(!crate::transaction::chemistry_changed(&original, &doc));
    depth::set_rear_opacity(&mut doc, &all, 1.).unwrap();
    assert_eq!(scene::svg(&doc), original_svg, "Opaque RGB output changed");
}

#[test]
fn view_visibility_budget_fallback_is_wholly_opaque_and_order_independent() {
    let source = cage(sources()[0]);
    let mut doc = source.clone();
    let source_ids = source.all_ids();
    for i in 1..5 {
        crate::editing::append(
            &mut doc,
            &crate::editing::selection(&source, &source_ids),
            Point::new(i as f32 * 1000., 0.),
        );
    }
    let paint = Paint::new(&doc);
    assert!(paint.is_empty());
    assert!(doc.atoms.iter().all(|a| paint.atom(a.id) == 1.));
    assert!(doc.bonds.iter().all(|b| paint.bond(b, 0.5) == 1.));
    doc.atoms.reverse();
    doc.bonds.reverse();
    assert!(Paint::new(&doc).is_empty());
}
#[test]
fn view_visibility_rim_backbone_and_inward_rail_have_independent_depth() {
    for source in sources() {
        let doc = cage(source);
        let paint = Paint::new(&doc);
        let hull = rim(&doc);
        let mut rim_hidden_rails = 0;
        let mut visible_rails = 0;
        let mut avoided_self_shadow = 0;
        let visibility = visibility::Visibility::new(&doc);
        for original in &doc.bonds {
            let mut b = original.clone();
            b.order = 2;
            b.double_position = crate::bonds::DoublePosition::Auto;
            let Some(support) = scene::projected_bonds::face_atoms(&doc, &b) else {
                continue;
            };
            let a = doc.atom(b.a).unwrap().position;
            let z = doc.atom(b.b).unwrap().position;
            let center =
                support
                    .iter()
                    .filter_map(|id| doc.atom(*id))
                    .fold(Point::default(), |p, a| {
                        p.offset(
                            a.position.x / support.len() as f32,
                            a.position.y / support.len() as f32,
                        )
                    });
            let side = if cross(a, z, center) < 0. { -1. } else { 1. };
            let spacing =
                doc.drawing_style.bond_length_world * doc.drawing_style.bond_spacing_ratio;
            let Some(Some(rail)) = scene::projected_bonds::rail_depth(
                &doc,
                &b,
                a,
                z,
                spacing * side,
                spacing * 0.75,
                doc.drawing_style.line_width() / 2.,
            ) else {
                continue;
            };
            let (first, last) = rail.points;
            let parts = paint.rail_parts(
                &doc,
                &b,
                &rail,
                vec![Primitive::Line(first, last, doc.drawing_style.line_width())],
            );
            if parts
                .iter()
                .any(|p| matches!(p,Primitive::Opacity{alpha,..} if *alpha==0.25))
            {
                if hull.contains(&b.a) && hull.contains(&b.b) && paint.bond(&b, 0.5) == 1. {
                    rim_hidden_rails += 1;
                    assert_eq!(paint.bond(&b, 0.), 1.);
                    assert_eq!(paint.bond(&b, 1.), 1.);
                }
            } else {
                visible_rails += 1;
                if !visibility
                    .support_ranges(first, last, rail.field, &rail.support)
                    .is_empty()
                {
                    avoided_self_shadow += 1;
                }
            }
        }
        println!(
            "C{}: {rim_hidden_rails} exposed backbones with hidden inward rails; {visible_rails} visible inward rails",
            doc.atoms.len()
        );
        assert!(
            rim_hidden_rails > 0,
            "Rear support exclusion accidentally exempted adjacent front faces"
        );
        assert!(visible_rails > 0, "All face annotations self-shadowed");
        println!(
            "C{}: {avoided_self_shadow} exposed rails protected from their mildly warped support face",
            doc.atoms.len()
        );
        if doc.atoms.len() == 70 {
            assert!(
                avoided_self_shadow > 0,
                "No actual C70 support-face warp case checked"
            );
        }
    }
}

#[test]
fn view_visibility_shell_respects_external_and_own_explicit_bond_layers() {
    let mut doc = cage(sources()[0]);
    let a = doc.add_atom("C", Point::new(-1., 0.));
    let b = doc.add_atom("C", Point::new(1., 0.));
    for id in [a, b] {
        doc.atom_mut(id).unwrap().depth = -200.;
    }
    doc.add_bond(a, b, 1, "plain");
    depth::set_rear_opacity(&mut doc, &[a, b], 0.25).unwrap();
    let index = doc.bonds.len() - 1;
    assert_eq!(Paint::new(&doc).bond(&doc.bonds[index], 0.5), 0.25);
    doc.bonds[index].z_order = 1;
    assert_eq!(Paint::new(&doc).bond(&doc.bonds[index], 0.5), 1.);
    doc.bonds[index].z_order = -1;
    for bond in &mut doc.bonds[..index] {
        bond.z_order = -2;
    }
    doc.bonds[0].z_order = -3;
    assert_eq!(
        Paint::new(&doc).bond(&doc.bonds[index], 0.5),
        1.,
        "Mixed negative cage layers acquired neutral priority"
    );
    for id in [a, b] {
        doc.atom_mut(id).unwrap().depth = 1000.;
    }
    doc.bonds[index].z_order = -4;
    assert_eq!(
        Paint::new(&doc).bond(&doc.bonds[index], 0.5),
        0.25,
        "Higher explicit layer lost precedence"
    );
    let mut own = cage(sources()[0]);
    let hidden = own
        .bonds
        .iter()
        .position(|b| Paint::new(&own).bond(b, 0.5) == 0.25)
        .unwrap();
    own.bonds[hidden].z_order = 1;
    assert_eq!(
        Paint::new(&own).bond(&own.bonds[hidden], 0.5),
        1.,
        "Bring forward on a rear cage bond remained behind its shell"
    );
}
#[test]
fn view_visibility_external_foreground_clips_cap_without_hitting_rim_anchor() {
    let mut doc = Document::default();
    let length = 100.;
    let ids: Vec<_> = [
        (0., 0., 0.),
        (length, length, 0.),
        (length, -length, 0.),
        (length / 2., 0., -length),
    ]
    .into_iter()
    .map(|(x, y, z)| {
        let id = doc.add_atom("C", Point::new(x, y));
        doc.atom_mut(id).unwrap().depth = z;
        id
    })
    .collect();
    for (a, b) in [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)] {
        doc.add_bond(ids[a], ids[b], 1, "plain");
    }
    depth::set_rear_opacity(&mut doc, &ids, 0.25).unwrap();
    assert_eq!(visibility::Visibility::new(&doc).face_count(), 4);
    let width = doc.drawing_style.line_width();
    let foreground: Vec<_> = [-2., 2.]
        .into_iter()
        .map(|y| {
            let id = doc.add_atom("C", Point::new(width * 0.75, width * y));
            doc.atom_mut(id).unwrap().depth = length;
            id
        })
        .collect();
    doc.add_bond(foreground[0], foreground[1], 1, "plain");
    let original = doc.clone();
    // Rasterize the actual opacity-split inward bond alone. The independent
    // foreground stroke is an occluder, not part of this underlying-ink image.
    fn raster(parts: &[Primitive], width: f32) -> resvg::tiny_skia::Pixmap {
        fn xml(p: &Primitive, alpha: f32) -> String {
            match p {
                Primitive::Opacity {
                    alpha: a,
                    primitive,
                } => xml(primitive, alpha * a),
                Primitive::Polygon(points) => format!(
                    "<polygon fill='black' fill-opacity='{alpha}' points='{}'/>",
                    points
                        .iter()
                        .map(|p| format!("{},{}", p.x, p.y))
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
                _ => panic!("Unexpected underlying joined bond primitive"),
            }
        }
        let svg = format!(
            "<svg xmlns='http://www.w3.org/2000/svg' width='1280' height='1024' viewBox='{} {} {} {}'>{}</svg>",
            -width,
            -2. * width,
            5. * width,
            4. * width,
            parts.iter().map(|p| xml(p, 1.)).collect::<String>()
        );
        let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).unwrap();
        let mut image = resvg::tiny_skia::Pixmap::new(1280, 1024).unwrap();
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::identity(),
            &mut image.as_mut(),
        );
        image
    }
    for layer in [-1, 0, 1] {
        doc.bonds.last_mut().unwrap().z_order = layer;
        for alpha in [0., 0.25] {
            depth::set_rear_opacity(&mut doc, &ids, alpha).unwrap();
            let paint = Paint::new(&doc);
            assert_eq!(paint.atom(ids[0]), 1., "Foreground misses the hull anchor");
            let bond = &doc.bonds[2];
            let hidden = paint.bond(bond, width * 0.75 / (length / 2.));
            assert_eq!(hidden, alpha, "Underlying cap centerline is hidden");
            let joins = crate::bond_joins::Joins::new(&doc);
            let outline = joins.polygon(
                bond,
                doc.atom(bond.a).unwrap().position,
                doc.atom(bond.b).unwrap().position,
            );
            let image = raster(
                &paint.bond_parts(&doc, bond, vec![Primitive::Polygon(outline)]),
                width,
            );
            let actual = image.pixel(448, 512).unwrap().alpha();
            let expected = if layer < 0 {
                255
            } else {
                (alpha * 255.).round() as u8
            };
            println!(
                "External cap foreground layer{layer}/alpha{alpha}: restored underlying alpha={actual}, expected={expected}"
            );
            assert!(
                i16::from(actual).abs_diff(i16::from(expected)) <= 1,
                "Opaque cap restoration overrode external foreground"
            );
            assert_eq!(
                image.pixel(282, 512).unwrap().alpha(),
                255,
                "Unoccluded anchor-owned cap sector faded"
            );
        }
    }
    assert_eq!(doc.atoms, original.atoms);
    assert!(!crate::transaction::chemistry_changed(&original, &doc));
}

#[test]
fn view_visibility_rim_exemption_does_not_leak_to_unrelated_foreground_ink() {
    let mut doc = cage(sources()[0]);
    let p = doc.atom(14).unwrap().position;
    assert_eq!(Paint::new(&doc).atom(14), 1.);
    let a = doc.add_atom("C", p.offset(-8., 0.));
    let b = doc.add_atom("C", p.offset(8., 0.));
    for id in [a, b] {
        doc.atom_mut(id).unwrap().depth = 500.;
    }
    doc.add_bond(a, b, 1, "plain");
    assert_eq!(
        Paint::new(&doc).atom(14),
        0.25,
        "Genuine external occluder was exempted with the rim's own cage"
    );
}
#[test]
fn view_visibility_centered_double_rails_do_not_erase_managed_rear_ink() {
    let mut doc = Document::default();
    let ids: Vec<_> = [
        (-50., 0., -10.),
        (50., 0., -10.),
        (0., -50., 10.),
        (0., 50., 10.),
    ]
    .into_iter()
    .map(|(x, y, z)| {
        let id = doc.add_atom("C", Point::new(x, y));
        doc.atom_mut(id).unwrap().depth = z;
        id
    })
    .collect();
    doc.add_bond(ids[0], ids[1], 1, "plain");
    doc.add_bond(ids[2], ids[3], 2, "plain");
    doc.bonds[1].double_position = crate::bonds::DoublePosition::Center;
    depth::set_rear_opacity(&mut doc, &ids[..2], 0.25).unwrap();
    let paint = Paint::new(&doc);
    assert_eq!(
        paint.bond(&doc.bonds[0], 0.5),
        1.,
        "The center lies between two real foreground rails"
    );
    let spacing = doc.drawing_style.bond_length_world * doc.drawing_style.bond_spacing_ratio;
    for x in [-spacing / 2., spacing / 2.] {
        assert_eq!(paint.bond(&doc.bonds[0], (x + 50.) / 100.), 0.25);
    }
    assert!(
        crate::crossings::gaps(&doc)[0].is_empty(),
        "Backbone-only knockout erased the exposed middle and faint rail intervals"
    );
    let parts = paint.bond_parts(
        &doc,
        &doc.bonds[0],
        vec![Primitive::Line(
            Point::new(-50., 0.),
            Point::new(50., 0.),
            1.,
        )],
    );
    assert!(
        parts
            .iter()
            .any(|p| matches!(p,Primitive::Opacity{alpha,..} if *alpha==0.25))
    );
    assert!(
        parts
            .iter()
            .filter(|p| !matches!(p, Primitive::Opacity { .. }))
            .count()
            >= 3
    );
}

#[test]
fn view_visibility_svg_raster_keeps_full_rim_stroke_and_real_hidden_alpha_on_both_themes() {
    use crate::canvas_theme::CanvasTheme;
    let scale = 8.;
    for theme in CanvasTheme::ALL {
        for alpha in [0., 0.25, 0.5, 1.] {
            let mut doc = cage(sources()[0]);
            doc.canvas_theme = theme;
            let ids = doc.all_ids();
            depth::set_rear_opacity(&mut doc, &ids, alpha).unwrap();
            let render = |doc: &Document| {
                let svg = scene::svg(doc);
                let tree =
                    resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).unwrap();
                let size = tree.size();
                let mut image = resvg::tiny_skia::Pixmap::new(
                    (size.width() * scale).ceil() as u32,
                    (size.height() * scale).ceil() as u32,
                )
                .unwrap();
                resvg::render(
                    &tree,
                    resvg::tiny_skia::Transform::from_scale(scale, scale),
                    &mut image.as_mut(),
                );
                let (lo, hi) = scene::bounds(&scene::primitives(doc));
                (image, lo, hi, size)
            };
            let (image, lo, hi, size) = render(&doc);
            let mut opaque = doc.clone();
            depth::set_rear_opacity(&mut opaque, &ids, 1.).unwrap();
            let (reference, ref_lo, ref_hi, ref_size) = render(&opaque);
            let pixel = |image: &resvg::tiny_skia::Pixmap,
                         p: Point,
                         lo: Point,
                         hi: Point,
                         size: resvg::usvg::Size| {
                let x = ((p.x - lo.x) / (hi.x - lo.x) * size.width() * scale).floor() as u32;
                let y = ((p.y - lo.y) / (hi.y - lo.y) * size.height() * scale).floor() as u32;
                image.pixel(x, y).unwrap().alpha()
            };
            let sample = |p| pixel(&image, p, lo, hi, size);
            let rear = Point::new(-8.102754, -18.966398);
            let actual = sample(rear);
            let expected = (alpha * 255.).round() as i16;
            assert!(
                (i16::from(actual) - expected).abs() <= 1,
                "Actual SVG hidden ink alpha {actual}/{alpha}/{theme:?}"
            );
            let hull = rim(&doc);
            let mut tested = 0;
            for (&first, &last) in hull
                .iter()
                .zip(hull.iter().cycle().skip(1))
                .take(hull.len())
            {
                assert!(
                    doc.bonds
                        .iter()
                        .any(|b| [b.a, b.b].contains(&first) && [b.a, b.b].contains(&last))
                );
                tested += 1;
                let a = doc.atom(first).unwrap().position;
                let b = doc.atom(last).unwrap().position;
                let length = a.distance(b);
                let normal = Point::new(-(b.y - a.y) / length, (b.x - a.x) / length);
                let width = doc.drawing_style.line_width();
                for t in [0., 0.1, 0.25, 0.5, 0.75, 0.9, 1.] {
                    for side in [-0.2, 0., 0.2] {
                        let p =
                            mix(a, b, t).offset(normal.x * width * side, normal.y * width * side);
                        let original = pixel(&reference, p, ref_lo, ref_hi, ref_size);
                        let actual = sample(p);
                        if first == 14 && t == 0. && side == 0.2 {
                            println!(
                                "Rim endpoint {theme:?}/{alpha}: original={original}, actual={actual}, targetbounds={lo:?}/{hi:?}, originalbounds={ref_lo:?}/{ref_hi:?}"
                            );
                        }
                        assert!(
                            i16::from(actual) >= i16::from(original) - 1,
                            "Rendered rim inner/outer/cap ink faded at {first}-{last}/{t}/{side}/{alpha}/{theme:?}: original {original}, actual {actual}"
                        );
                        if t > 0. && t < 1. {
                            assert!(
                                original >= 250,
                                "Rim body probe was not inside the original opaque stroke"
                            );
                        }
                    }
                }
            }
            assert_eq!(tested, 20);
            if let Ok(directory) = std::env::var("RESHIKI_VISIBILITY_TEST_OUTPUT") {
                let path =
                    std::path::Path::new(&directory).join(format!("c60-{theme:?}-{alpha}.png"));
                image.save_png(path).unwrap();
                let svg = scene::svg_with_background(&doc);
                let tree =
                    resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).unwrap();
                let mut paper =
                    resvg::tiny_skia::Pixmap::new(image.width(), image.height()).unwrap();
                resvg::render(
                    &tree,
                    resvg::tiny_skia::Transform::from_scale(scale, scale),
                    &mut paper.as_mut(),
                );
                paper
                    .save_png(
                        std::path::Path::new(&directory)
                            .join(format!("c60-{theme:?}-{alpha}-paper.png")),
                    )
                    .unwrap();
            }
        }
    }
}

#[test]
fn view_visibility_exposed_miter_sectors_retain_opaque_compound_ink() {
    let mut doc = cage(sources()[0]);
    let ids = doc.all_ids();
    depth::set_rear_opacity(&mut doc, &ids, 0.).unwrap();
    let a = doc.atom(13).unwrap().position;
    let b = doc.atom(5).unwrap().position;
    let length = a.distance(b);
    let p = mix(a, b, 0.1).offset(
        -(b.y - a.y) / length * doc.drawing_style.line_width() * 0.2,
        (b.x - a.x) / length * doc.drawing_style.line_width() * 0.2,
    );
    for alpha in [0., 1.] {
        depth::set_rear_opacity(&mut doc, &ids, alpha).unwrap();
        let mut summaries = vec![];
        for primitive in scene::primitives(&doc) {
            if let Primitive::Path {
                commands,
                filled: true,
                ..
            } = primitive
            {
                let contours = flatten(&commands);
                let mut positive = 0;
                let mut negative = 0;
                let mut winding = 0;
                for contour in contours {
                    let area = contour
                        .iter()
                        .zip(contour.iter().cycle().skip(1))
                        .take(contour.len())
                        .map(|(a, b)| {
                            f64::from(a.x) * f64::from(b.y) - f64::from(a.y) * f64::from(b.x)
                        })
                        .sum::<f64>();
                    if area > 0. {
                        positive += 1;
                    } else if area < 0. {
                        negative += 1;
                    }
                    for (a, b) in contour
                        .iter()
                        .zip(contour.iter().cycle().skip(1))
                        .take(contour.len())
                    {
                        if a.y <= p.y && b.y > p.y && cross(*a, *b, p) > 0. {
                            winding += 1;
                        }
                        if a.y > p.y && b.y <= p.y && cross(*a, *b, p) < 0. {
                            winding -= 1;
                        }
                    }
                }
                summaries.push((positive, negative, winding));
            }
        }
        println!(
            "Compound ink alpha{alpha}, rim13-5 body probe{p:?}: (positive contours,negative contours,winding)={summaries:?}"
        );
        assert!(
            summaries.iter().any(|(_, _, winding)| *winding != 0),
            "Shared exposed miter sector disappeared"
        );
    }
}

#[test]
fn view_visibility_degenerate_cap_masks_never_restore_an_entire_hidden_bond() {
    let polygon = vec![
        Point::new(-5., -1.),
        Point::new(5., -1.),
        Point::new(5., 1.),
        Point::new(-5., 1.),
    ];
    let line = vec![Point::new(-1., 0.), Point::default(), Point::new(1., 0.)];
    assert!(mask_clip(&polygon, &line, false).is_empty());
    assert_eq!(mask_clip(&polygon, &line, true), vec![polygon.clone()]);
    assert!(mask_clip(&polygon, &line[..2], false).is_empty());
}
