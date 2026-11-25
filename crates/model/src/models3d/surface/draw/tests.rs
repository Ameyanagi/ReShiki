use super::*;
use crate::document::Point;
use crate::graphics::PathCommand;
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

/// Whether `p` lies inside a fitted phase region.
fn covered(surface: &Primitive, p: Point) -> bool {
    let Primitive::Surface { groups } = surface else {
        return false;
    };
    groups.iter().flat_map(|g| &g.fills).any(|fill| {
        let (mut start, mut from) = (Point::default(), Point::default());
        let mut winding = 0;
        let mut edge = |a: Point, b: Point| {
            if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
                winding += if a.y < b.y { 1 } else { -1 };
            }
        };
        for command in &fill.region {
            match *command {
                PathCommand::Move(a) => (start, from) = (a, a),
                PathCommand::Line(to) => {
                    edge(from, to);
                    from = to;
                }
                PathCommand::Cubic(b, c, to) => {
                    let a = from;
                    for step in 1..=16 {
                        let t = step as f32 / 16.;
                        let u = 1. - t;
                        let coord = |a, b, c, d| {
                            u * u * u * a + 3. * u * u * t * b + 3. * u * t * t * c + t * t * t * d
                        };
                        let next =
                            Point::new(coord(a.x, b.x, c.x, to.x), coord(a.y, b.y, c.y, to.y));
                        edge(from, next);
                        from = next;
                    }
                }
                PathCommand::Close => {
                    edge(from, start);
                    from = start;
                }
            }
        }
        winding != 0
    })
}

fn groups(doc: &Document, spec: &str) -> Vec<SurfaceGroup> {
    demo_on_this_thread(Some(spec)).unwrap();
    let mut surfaces = surfaces(doc);
    demo_on_this_thread(None).unwrap();
    assert_eq!(surfaces.len(), 1, "{spec}");
    let Primitive::Surface { groups } = surfaces.remove(0) else {
        unreachable!()
    };
    groups
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
    for (spec, message) in [
        ("sigma", "Unknown demo"),
        ("pi:0", "Isovalue"),
        ("pi:-1", "Isovalue"),
        ("pi:nan", "Isovalue"),
        ("pistar:x", "Isovalue"),
        ("pi,look=smooth", "Unknown surface look"),
        ("pi,look=mesh+mesh", "Unknown surface look"),
        ("pi,opacity=19", "opacity"),
        ("pi,opacity=101", "opacity"),
        ("pi,opacity=0.6", "opacity"),
        ("pi,alpha=50", "Unknown surface option"),
        ("pi,mesh", "key=value"),
        ("pi,look=solid,look=mesh", "Duplicate surface option"),
    ] {
        let error = demo_on_this_thread(Some(spec)).unwrap_err();
        assert!(error.contains(message), "{spec}: {error}");
    }
    for (spec, expected) in [
        ("pi", (Demo::Pi, 0.06, Look::default())),
        ("pistar:0.05", (Demo::PiStar, 0.05, Look::default())),
        (
            "pistar:0.06,look=shaded+mesh,opacity=60",
            (
                Demo::PiStar,
                0.06,
                Look::new(Fill::Shaded, true, 60).unwrap(),
            ),
        ),
    ] {
        demo_on_this_thread(Some(spec)).unwrap();
        assert_eq!(switch(), Some(expected));
    }
    demo_on_this_thread(None).unwrap();
}

#[test]
fn changing_the_look_reuses_the_mesh() {
    let doc = ethylene();
    demo_on_this_thread(Some("pi")).unwrap();
    let shaded = meshes(&doc, &doc.models[0]).unwrap();
    demo_on_this_thread(Some("pi,look=mesh,opacity=30")).unwrap();
    let mesh = meshes(&doc, &doc.models[0]).unwrap();
    demo_on_this_thread(None).unwrap();
    assert!(Rc::ptr_eq(&shaded, &mesh));
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
    let Primitive::Surface { groups } = &all[at] else {
        unreachable!()
    };
    let [positive, negative] = groups[0].fills.as_slice() else {
        panic!("Expected two phases");
    };
    let blue = |[r, _, b]: [u8; 3]| i32::from(b) > i32::from(r) + 40;
    assert!(blue(positive.color) && !blue(negative.color));
    assert!(!groups[0].strokes.is_empty());
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
fn export_estimate_counts_the_surface_and_stays_under_the_warning() {
    demo_on_this_thread(None).unwrap();
    let doc = tilted(benzene(), 50.);
    let (plain, ..) = super::super::export_estimate(&doc);
    demo_on_this_thread(Some("pistar")).unwrap();
    let (svg, pdf, warn) = super::super::export_estimate(&doc);
    assert_eq!(svg, scene::svg(&doc).len());
    assert!(svg > plain && pdf >= svg && !warn, "{plain} {svg} {pdf}");
    demo_on_this_thread(None).unwrap();
}

#[test]
fn the_plane_normal_is_the_direction_of_least_spread() {
    let square = [[1., 1., 0.], [1., -1., 0.], [-1., 1., 0.], [-1., -1., 0.]];
    assert!(plane_normal(&square)[2].abs() > 0.999);
    let turned = square.map(|[x, y, z]| [x, 0.6 * y - 0.8 * z, 0.8 * y + 0.6 * z]);
    let n = plane_normal(&turned);
    assert!((n[1] * -0.8 + n[2] * 0.6).abs() > 0.999, "{n:?}");
}

#[test]
fn a_label_behind_another_ball_stays_behind_it() {
    demo_on_this_thread(Some("pi")).unwrap();
    let mut doc = planar(
        &[
            ("O", [-0.8, 0., 0.]),
            ("C", [0., 1.4, 0.]),
            ("N", [0.8, 0., 0.]),
        ],
        &[(0, 1, 1), (1, 2, 1)],
        &[],
    );
    doc.models[0].labels = Labels::All;
    doc.models[0].perspective = 0.;
    // A quarter turn about y puts N straight behind O.
    let half = std::f32::consts::FRAC_PI_4;
    doc.models[0].rotation = [half.cos(), 0., half.sin(), 0.];
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
    assert!(!late.contains(&"N"), "{late:?}");
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
    let Primitive::Surface { groups } = &surfaces(&doc)[0] else {
        unreachable!()
    };
    let fills = groups.iter().flat_map(|g| &g.fills).flat_map(|fill| {
        std::iter::once(&fill.region).chain(fill.tones.iter().map(|(path, _)| path))
    });
    let strokes = groups.iter().flat_map(|g| &g.strokes).map(|s| &s.paths);
    for p in fills
        .chain(strokes)
        .flatten()
        .flat_map(PathCommand::iter_points)
    {
        assert!(
            p.x >= lo.x && p.y >= lo.y && p.x <= hi.x && p.y <= hi.y,
            "{p:?}"
        );
    }
    demo_on_this_thread(None).unwrap();
}

#[test]
fn looks_paint_the_requested_fills_and_strokes() {
    let doc = tilted(ethylene(), 55.);
    let shaded = groups(&doc, "pi,look=shaded");
    assert_eq!(shaded.len(), 1);
    assert_eq!(shaded[0].alpha, 1.);
    let positive = &shaded[0].fills[0];
    assert!(positive.tones.len() >= 60, "{}", positive.tones.len());
    // The endpoint range bounds the average step. The steeper part of the
    // exact piecewise ramp can round to two levels between uniform midpoints.
    for (a, b) in std::iter::once(positive.color)
        .chain(positive.tones.iter().map(|(_, color)| *color))
        .zip(positive.tones.iter().map(|(_, color)| *color))
    {
        assert!(
            a.into_iter().zip(b).all(|(a, b)| a.abs_diff(b) <= 2),
            "{a:?} → {b:?}"
        );
    }

    let solid = groups(&doc, "pi,look=solid");
    assert!(!solid[0].fills.is_empty());
    assert!(solid[0].fills.iter().all(|fill| fill.tones.is_empty()));

    let outline = groups(&doc, "pi,look=outline");
    let backdrop = match doc.models[0].background {
        crate::models3d::Background::None => doc.canvas_theme.background(),
        crate::models3d::Background::Card => crate::models3d::CARD,
    };
    assert!(!outline[0].fills.is_empty());
    assert!(outline[0].fills.iter().all(|fill| fill.color == backdrop));

    let mesh = groups(&doc, "pi,look=mesh");
    assert!(mesh[0].fills.is_empty());
    assert!(
        mesh[0]
            .strokes
            .iter()
            .any(|stroke| stroke.outline.alpha == 0.9)
    );

    let both = groups(&doc, "pi,look=shaded+mesh");
    assert_eq!(both[0].fills, shaded[0].fills);
    let (rim, mesh) = both[0].strokes.split_last().unwrap();
    assert_eq!(rim, &shaded[0].strokes[0]);
    assert!(!mesh.is_empty());
    assert!(mesh.iter().all(|stroke| stroke.outline.alpha == 0.9));

    let mut glossy = doc;
    glossy.models[0].look = crate::models3d::Look::Glossy;
    let line_width = (glossy.models[0].size / 400.).clamp(0.6, 1.1);
    let mesh = groups(&glossy, "pi,look=mesh");
    assert!(!mesh[0].strokes.is_empty());
    assert!(mesh[0].strokes.iter().all(|stroke| {
        stroke.outline.alpha == 0.9 && stroke.outline.width == 0.45 * line_width
    }));
}

#[test]
fn tone_counts_follow_the_faded_endpoint_range_and_group_weights() {
    let backdrop = [255.; 3];
    let fade = 0.175;
    assert_eq!(shaded(POSITIVE, 0.35, backdrop, fade), [86, 119, 188]);
    assert_eq!(shaded(POSITIVE, 1., backdrop, fade), [175, 196, 238]);
    // The ramp bends at lightness 0.6: 33 levels below it (blue), 80 above (red).
    assert_eq!(tone_counts(Look::default(), false, backdrop, fade), [114]);
    let film = Look::new(Fill::Shaded, false, 60).unwrap();
    assert_eq!(tone_counts(film, false, backdrop, fade), [68, 28, 11, 5]);
}

#[test]
fn translucent_groups_put_the_nearest_layer_last() {
    let doc = tilted(benzene(), 55.);
    let opaque = groups(&doc, "pistar");
    let film = groups(&doc, "pistar,opacity=60");
    assert!(film.len() > 1);
    assert!(film.iter().all(|group| group.alpha == 0.6));
    assert_eq!(
        film.last().unwrap().fills[0].region,
        opaque[0].fills[0].region
    );

    let mesh = groups(&doc, "pistar,look=mesh,opacity=30");
    assert!(mesh.len() >= 3);
    for (group, alpha) in mesh
        .iter()
        .rev()
        .zip([0.3, 0.21, 0.147, 0.1029, 0.07203, 0.050421])
    {
        assert!(group.fills.is_empty());
        assert!((group.alpha - alpha).abs() < 1e-6, "{}", group.alpha);
    }
}

#[test]
fn svg_composites_groups_with_clipped_vector_tones() {
    demo_on_this_thread(Some("pistar,opacity=60")).unwrap();
    let svg = scene::svg(&tilted(benzene(), 55.));
    demo_on_this_thread(None).unwrap();
    assert!(svg.contains("<g opacity=\"0.6\">"));
    assert!(svg.contains("<clipPath id=\"reshiki-surface-"));
    assert!(svg.contains("fill-rule=\"nonzero\""));
    assert!(svg.contains("clip-rule=\"nonzero\""));
    assert!(svg.split("<path d=\"").skip(1).any(|element| {
        element.split_once('"').is_some_and(|(path, rest)| {
            path.contains('c')
                && rest
                    .split('>')
                    .next()
                    .is_some_and(|attributes| attributes.contains("fill-rule=\"nonzero\""))
        })
    }));
    assert!(!svg.contains("<image"));
}

#[test]
fn opaque_shaded_svg_preserves_fitted_region_commands() {
    demo_on_this_thread(Some("pi")).unwrap();
    let doc = ethylene();
    for degrees in [0., 55.] {
        let surfaces = surfaces(&tilted(doc.clone(), degrees));
        let Primitive::Surface { groups } = &surfaces[0] else {
            unreachable!()
        };
        let mut expected = (0, 0);
        for command in groups[0].fills.iter().flat_map(|fill| &fill.region) {
            match command {
                PathCommand::Cubic(..) => expected.0 += 1,
                PathCommand::Line(..) => expected.1 += 1,
                PathCommand::Move(..) | PathCommand::Close => {}
            }
        }
        let svg = scene::svg_part(&surfaces, Point::new(-100., -100.), Point::new(100., 100.));
        let (mut regions, mut cubics, mut lines) = (0, 0, 0);
        // Each clip repeats its exact phase region path.
        for clip in svg.split("<clipPath id=\"reshiki-surface-").skip(1) {
            let (_, path) = clip.split_once("<path d=\"").unwrap();
            let (data, _) = path.split_once('"').unwrap();
            regions += 1;
            // Relative commands: lower case.
            cubics += data.matches('c').count();
            lines += data.matches('l').count();
        }
        assert_eq!(regions, groups[0].fills.len());
        assert_eq!((cubics, lines), expected, "{degrees}°");
        if degrees == 0. {
            assert!(
                cubics > lines,
                "{cubics} cubic commands, {lines} line commands"
            );
        }
    }
    demo_on_this_thread(None).unwrap();
}

/// Reports fitting quality for the shaded benzene π* surface, including
/// every phase region, tone path and surface stroke.
#[test]
#[ignore]
fn benzene_fit_diagnostic() {
    fn counts<'a>(commands: impl Iterator<Item = &'a PathCommand>) -> [usize; 5] {
        let mut counts = [0; 5];
        for command in commands {
            counts[0] += 1;
            counts[match command {
                PathCommand::Cubic(..) => 1,
                PathCommand::Line(_) => 2,
                PathCommand::Move(_) => 3,
                PathCommand::Close => 4,
            }] += 1;
        }
        counts
    }

    let _ = fit::take_diagnostics();
    let groups = groups(&tilted(benzene(), 55.), "pistar");
    let diagnostics = fit::take_diagnostics();
    let fills = counts(
        groups
            .iter()
            .flat_map(|group| &group.fills)
            .flat_map(|fill| {
                std::iter::once(&fill.region).chain(fill.tones.iter().map(|(path, _)| path))
            })
            .flatten(),
    );
    let strokes = counts(
        groups
            .iter()
            .flat_map(|group| &group.strokes)
            .flat_map(|stroke| &stroke.paths),
    );
    for ((label, [commands, cubics, lines, moves, closes]), (corners, fallbacks)) in
        [("fills (regions + tones)", fills), ("strokes", strokes)]
            .into_iter()
            .zip(diagnostics)
    {
        println!(
            "{label}: {commands} commands ({moves} moves, {closes} closes), {cubics} cubics, {lines} lines, {corners} corners detected, {fallbacks} topology-fallback contours"
        );
    }

    let [commands, cubics, lines, ..] = fills;
    assert!(cubics + lines > 0 && strokes[0] > 0);
    assert!(
        lines * 100 <= (cubics + lines) * 5,
        "fills: {lines} of {} segments are lines, exceeding 5%",
        cubics + lines,
    );
    assert!(
        commands <= 36_311,
        "{commands} fill commands exceed budget 36311"
    );
}

/// Release-mode frame and export budgets at drawing detail. Run with
/// `cargo test --release -p reshiki-model export_budgets -- --ignored --nocapture`.
#[test]
#[ignore]
fn export_budgets() {
    use std::time::Instant;
    println!(
        "| molecule | demo | look | triangles | frame ms | groups | group 0 tones | path commands | SVG KB | snapshot KB |"
    );
    println!("|---|---|---|---:|---:|---:|---:|---:|---:|---:|");
    for (name, doc) in [("ethylene", ethylene()), ("benzene", benzene())] {
        let doc = tilted(doc, 55.);
        for demo in ["pi", "pistar"] {
            demo_on_this_thread(Some(demo)).unwrap();
            MESHES.with_borrow_mut(Vec::clear);
            let meshes = meshes(&doc, &doc.models[0]).unwrap();
            for (look, opacity) in [
                ("shaded", 100),
                ("shaded+mesh", 100),
                ("shaded", 60),
                ("mesh", 100),
                ("mesh", 30),
                ("solid", 100),
                ("outline", 100),
            ] {
                demo_on_this_thread(Some(&format!("{demo},look={look},opacity={opacity}")))
                    .unwrap();
                let started = Instant::now();
                let all = scene::primitives(&doc);
                let frame_ms = started.elapsed().as_secs_f32() * 1000.;
                let svg = scene::svg_of(&doc, all.clone(), None);
                let surface = all
                    .iter()
                    .find(|p| matches!(p, Primitive::Surface { .. }))
                    .unwrap();
                let Primitive::Surface { groups } = surface else {
                    unreachable!()
                };
                let tones: usize = groups
                    .last()
                    .unwrap()
                    .fills
                    .iter()
                    .map(|fill| fill.tones.len())
                    .sum();
                let commands: usize = groups
                    .iter()
                    .map(|group| {
                        group
                            .fills
                            .iter()
                            .map(|f| {
                                f.region.len()
                                    + f.tones.iter().map(|(path, _)| path.len()).sum::<usize>()
                            })
                            .sum::<usize>()
                            + group.strokes.iter().map(|s| s.paths.len()).sum::<usize>()
                    })
                    .sum();
                let snapshot = super::super::snapshot::encode(surface, Point::default()).unwrap();
                println!(
                    "| {name} | {demo} | {look} {opacity} | {} | {frame_ms:.1} | {} | {tones} | {commands} | {:.1} | {:.1} |",
                    meshes[0].triangles.len(),
                    groups.len(),
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

/// Writes the demo surfaces' SVG for each look to
/// `RESHIKI_SURFACE_EVIDENCE_DIR`, for rendering them in other viewers.
#[test]
#[ignore]
fn write_look_svgs() {
    let dir = std::path::PathBuf::from(std::env::var("RESHIKI_SURFACE_EVIDENCE_DIR").unwrap());
    for (name, demo, doc) in [
        ("ethylene", "pi", tilted(ethylene(), 55.)),
        ("benzene", "pistar", tilted(benzene(), 55.)),
    ] {
        for look in [
            "",
            ",look=solid",
            ",look=outline",
            ",look=mesh",
            ",look=shaded+mesh",
            ",opacity=60",
            ",opacity=30",
            ",look=mesh,opacity=30",
        ] {
            demo_on_this_thread(Some(&format!("{demo}{look}"))).unwrap();
            let file = format!("{name}{}.svg", look.replace([',', '=', '+'], "-"));
            std::fs::write(dir.join(file), scene::svg(&doc)).unwrap();
        }
    }
    demo_on_this_thread(None).unwrap();
}
