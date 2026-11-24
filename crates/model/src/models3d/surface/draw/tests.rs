use super::*;
use crate::models3d::{
    Display, ForceField, Hydrogen, Labels, Placed, require_version, set_display,
};
use crate::scene;

/// A planar molecule: heavy atoms with drawing places and 3D positions in
/// the xy plane, hydrogens on `parents`, and its bonds.
fn planar(
    heavy: &[(&str, [f32; 3])],
    bonds: &[(usize, usize, u8)],
    hydrogens: &[(usize, [f32; 3])],
) -> Document {
    let mut doc = Document::default();
    let ids: Vec<u64> = heavy
        .iter()
        .map(|(s, p)| doc.add_atom(s, Point::new(p[0] * 30., -p[1] * 30.)))
        .collect();
    for &(a, b, order) in bonds {
        doc.add_bond(ids[a], ids[b], order, "plain");
    }
    let atoms = heavy
        .iter()
        .zip(&ids)
        .map(|((_, xyz), &id)| Placed { id, xyz: *xyz })
        .collect();
    let hydrogens = hydrogens
        .iter()
        .map(|&(parent, xyz)| Hydrogen {
            parent: ids[parent],
            xyz,
        })
        .collect();
    doc.models
        .push(Model3d::new(&doc, atoms, hydrogens, ForceField::Mmff94s));
    require_version(&mut doc);
    assert!(set_display(&mut doc, 0, Display::ThreeD));
    doc
}

pub(crate) fn ethylene() -> Document {
    planar(
        &[("C", [-0.665, 0., 0.]), ("C", [0.665, 0., 0.])],
        &[(0, 1, 2)],
        &[
            (0, [-1.23, 0.92, 0.]),
            (0, [-1.23, -0.92, 0.]),
            (1, [1.23, 0.92, 0.]),
            (1, [1.23, -0.92, 0.]),
        ],
    )
}

pub(crate) fn benzene() -> Document {
    let at = |k: usize, r: f32| {
        let t = std::f32::consts::TAU * k as f32 / 6.;
        [r * t.cos(), r * t.sin(), 0.]
    };
    let heavy: Vec<(&str, [f32; 3])> = (0..6).map(|k| ("C", at(k, 1.39))).collect();
    let bonds: Vec<(usize, usize, u8)> = (0..6)
        .map(|k| (k, (k + 1) % 6, 1 + (k % 2) as u8))
        .collect();
    let hydrogens: Vec<(usize, [f32; 3])> = (0..6).map(|k| (k, at(k, 2.47))).collect();
    planar(&heavy, &bonds, &hydrogens)
}

fn surfaces(doc: &Document) -> Vec<Primitive> {
    scene::primitives(doc)
        .into_iter()
        .filter(|p| matches!(p, Primitive::Surface { .. }))
        .collect()
}

/// Turns the model so the molecule's plane makes `degrees` with the screen
/// about the x axis.
fn tilted(mut doc: Document, degrees: f32) -> Document {
    let half = degrees.to_radians() / 2.;
    doc.models[0].rotation = [half.cos(), half.sin(), 0., 0.];
    doc
}

/// Whether `p` lies inside the first (whole) layer of either phase.
fn covered(surface: &Primitive, p: Point) -> bool {
    let Primitive::Surface { layers, .. } = surface else {
        return false;
    };
    let inside = |contour: &Vec<Point>| {
        let mut odd = false;
        for (a, b) in contour.iter().zip(contour.iter().cycle().skip(1)) {
            if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
                odd = !odd;
            }
        }
        odd
    };
    // The first layer of each phase (blue positive, grey negative) covers its whole region.
    let blue = |c: &[u8; 3]| i32::from(c[2]) > i32::from(c[0]) + 40;
    [true, false].iter().any(|&positive| {
        layers
            .iter()
            .find(|(_, c)| blue(c) == positive)
            .is_some_and(|(region, _)| region.iter().filter(|c| inside(c)).count() % 2 == 1)
    })
}

#[test]
fn without_the_switch_nothing_changes() {
    demo_on_this_thread(None).unwrap();
    let doc = ethylene();
    assert!(surfaces(&doc).is_empty());
    let before = scene::svg(&doc);
    demo_on_this_thread(Some("pi")).unwrap();
    assert_eq!(surfaces(&doc).len(), 1);
    demo_on_this_thread(None).unwrap();
    assert_eq!(scene::svg(&doc), before);
}

#[test]
fn the_switch_rejects_bad_specs() {
    for spec in ["sigma", "pi:0", "pi:-1", "pi:nan", "pistar:x"] {
        assert!(demo_on_this_thread(Some(spec)).is_err(), "{spec}");
    }
    demo_on_this_thread(Some("pistar:0.04")).unwrap();
    demo_on_this_thread(None).unwrap();
}

#[test]
fn the_surface_is_drawn_after_the_molecule_and_holds_both_phases() {
    demo_on_this_thread(Some("pi")).unwrap();
    let doc = tilted(ethylene(), 60.);
    let all = scene::primitives(&doc);
    let at = all
        .iter()
        .position(|p| matches!(p, Primitive::Surface { .. }))
        .unwrap();
    assert!(
        all[..at]
            .iter()
            .any(|p| matches!(p, Primitive::Sphere { .. }))
    );
    assert!(
        !all[at..]
            .iter()
            .any(|p| matches!(p, Primitive::Sphere { .. } | Primitive::ShadedQuad { .. }))
    );
    let Primitive::Surface {
        layers,
        rims,
        outline,
    } = &all[at]
    else {
        unreachable!()
    };
    let blue = layers
        .iter()
        .filter(|(_, c)| i32::from(c[2]) > i32::from(c[0]) + 40)
        .count();
    assert!(
        blue >= 3 && blue < layers.len(),
        "{blue} of {}",
        layers.len()
    );
    assert!(!rims.is_empty() && outline.is_some());
    demo_on_this_thread(None).unwrap();
}

#[test]
fn a_lobe_hides_the_atoms_face_on_and_leaves_them_edge_on() {
    demo_on_this_thread(Some("pi")).unwrap();
    for (degrees, hidden) in [(0., true), (90., false)] {
        let doc = tilted(ethylene(), degrees);
        let (balls, _) = crate::models3d::balls_and_sticks(&doc, &doc.models[0]).unwrap();
        let surface = &surfaces(&doc)[0];
        // The two carbons come first in geometry order.
        for (centre, _) in &balls[..2] {
            assert_eq!(covered(surface, *centre), hidden, "{degrees}°");
        }
    }
    demo_on_this_thread(None).unwrap();
}

#[test]
fn labels_in_front_of_the_surface_come_after_it() {
    demo_on_this_thread(Some("pi")).unwrap();
    let mut doc = tilted(ethylene(), 90.);
    doc.models[0].labels = Labels::All;
    let all = scene::primitives(&doc);
    let at = all
        .iter()
        .position(|p| matches!(p, Primitive::Surface { .. }))
        .unwrap();
    // Edge-on, nothing of the lobes lies in front of the atoms.
    assert!(
        all[at..]
            .iter()
            .any(|p| matches!(p, Primitive::Text { .. }))
    );
    // Face-on, the front lobe covers the carbons, so theirs stay behind it.
    let doc = {
        let mut d = tilted(ethylene(), 0.);
        d.models[0].labels = Labels::All;
        d
    };
    let all = scene::primitives(&doc);
    let at = all
        .iter()
        .position(|p| matches!(p, Primitive::Surface { .. }))
        .unwrap();
    let late: Vec<_> = all[at..]
        .iter()
        .filter_map(|p| match p {
            Primitive::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(!late.contains(&"C"), "{late:?}");
    demo_on_this_thread(None).unwrap();
}

#[test]
fn an_isovalue_above_the_peak_draws_nothing_and_keeps_labels_in_place() {
    let mut doc = tilted(ethylene(), 90.);
    doc.models[0].labels = Labels::All;
    demo_on_this_thread(None).unwrap();
    let plain = scene::primitives(&doc);
    demo_on_this_thread(Some("pi:1000")).unwrap();
    let all = scene::primitives(&doc);
    assert!(surfaces(&doc).is_empty());
    let texts = |all: &[Primitive]| {
        all.iter()
            .map(|p| matches!(p, Primitive::Text { .. }))
            .collect::<Vec<_>>()
    };
    assert_eq!(texts(&all), texts(&plain));
    demo_on_this_thread(None).unwrap();
}

#[test]
fn unsupported_models_get_no_surface() {
    demo_on_this_thread(Some("pi")).unwrap();
    for style in [Style::Wireframe, Style::Ellipsoids] {
        let mut doc = ethylene();
        doc.models[0].style = style;
        assert!(surfaces(&doc).is_empty(), "{style:?}");
    }
    let mut doc = ethylene();
    doc.models[0].morph = Some(0.5);
    assert!(surfaces(&doc).is_empty());
    demo_on_this_thread(None).unwrap();
}

#[test]
fn the_frame_holds_the_surface() {
    demo_on_this_thread(None).unwrap();
    let doc = ethylene();
    let plain = crate::models3d::frame(&doc, &doc.models[0]).unwrap();
    demo_on_this_thread(Some("pi")).unwrap();
    let (lo, hi) = crate::models3d::frame(&doc, &doc.models[0]).unwrap();
    // Same frame; the model draws smaller inside it.
    assert_eq!((lo, hi), plain);
    let Primitive::Surface { layers, rims, .. } = &surfaces(&doc)[0] else {
        unreachable!()
    };
    for p in layers.iter().flat_map(|(r, _)| r).chain(rims).flatten() {
        assert!(
            p.x >= lo.x && p.y >= lo.y && p.x <= hi.x && p.y <= hi.y,
            "{p:?}"
        );
    }
    demo_on_this_thread(None).unwrap();
}

#[test]
fn svg_has_only_vector_paths() {
    demo_on_this_thread(Some("pistar")).unwrap();
    let svg = scene::svg(&tilted(benzene(), 50.));
    assert!(svg.contains("fill-rule=\"evenodd\""));
    assert!(!svg.contains("<image"));
    demo_on_this_thread(None).unwrap();
}

/// Release-mode numbers for the PR: mesh build, frame, SVG and snapshot
/// bytes at both details, for each demo molecule. Run with
/// `cargo test --release -p reshiki-model export_budgets -- --ignored --nocapture`.
#[test]
#[ignore]
fn export_budgets() {
    use std::time::Instant;
    println!(
        "| molecule | demo | detail | triangles | mesh ms | frame ms | layers | points | SVG KB | snapshot KB |"
    );
    println!("|---|---|---|---:|---:|---:|---:|---:|---:|---:|");
    for (name, doc) in [("ethylene", ethylene()), ("benzene", benzene())] {
        let doc = tilted(doc, 55.);
        for demo in ["pi", "pistar"] {
            demo_on_this_thread(Some(demo)).unwrap();
            MESHES.with_borrow_mut(Vec::clear);
            let started = Instant::now();
            let meshes = meshes(&doc, &doc.models[0]).unwrap();
            let mesh_ms = started.elapsed().as_secs_f32() * 1000.;
            for (k, turning) in [false, true].into_iter().enumerate() {
                let mut doc = doc.clone();
                doc.models[0].turning = turning;
                let started = Instant::now();
                let all = scene::primitives(&doc);
                let frame_ms = started.elapsed().as_secs_f32() * 1000.;
                let svg = scene::svg_of(&doc, all.clone(), None);
                let surface = all
                    .iter()
                    .find(|p| matches!(p, Primitive::Surface { .. }))
                    .unwrap();
                let Primitive::Surface { layers, rims, .. } = surface else {
                    unreachable!()
                };
                let points: usize = layers
                    .iter()
                    .flat_map(|(r, _)| r)
                    .chain(rims)
                    .map(Vec::len)
                    .sum();
                let snapshot = super::super::snapshot::encode(surface, Point::default()).unwrap();
                println!(
                    "| {name} | {demo} | {} | {} | {mesh_ms:.0} | {frame_ms:.0} | {} | {points} | {:.0} | {:.0} |",
                    DETAIL[k],
                    meshes[k].triangles.len(),
                    layers.len(),
                    svg.len() as f32 / 1024.,
                    snapshot.len() as f32 / 1024.,
                );
            }
        }
    }
    demo_on_this_thread(None).unwrap();
}

/// Writes the evidence documents (ethylene and benzene as 3D models) to
/// `RESHIKI_SURFACE_EVIDENCE_DIR`.
#[test]
#[ignore]
fn write_evidence_documents() {
    let dir = std::path::PathBuf::from(std::env::var("RESHIKI_SURFACE_EVIDENCE_DIR").unwrap());
    for (name, doc) in [
        ("ethylene", tilted(ethylene(), 55.)),
        ("benzene", tilted(benzene(), 55.)),
    ] {
        std::fs::write(dir.join(format!("{name}.rsk")), doc.file_json().unwrap()).unwrap();
    }
}
