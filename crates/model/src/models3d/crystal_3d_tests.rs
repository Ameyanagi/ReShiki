use super::*;
use crate::scene;

/// The eight stored NaCl sites from the rock-salt fixture, before display copies.
fn salt() -> Document {
    let mut doc = Document::default();
    let mut atoms = vec![];
    for x in 0..2 {
        for y in 0..2 {
            for z in 0..2 {
                let symbol = if (x + y + z) % 2 == 0 { "Na" } else { "Cl" };
                let id = doc.add_atom(symbol, Point::new(x as f32 * 40., y as f32 * 40.));
                atoms.push(Placed {
                    id,
                    xyz: [x, y, z].map(|v| v as f32 * 2.8201),
                });
            }
        }
    }
    let mut cell = Cell {
        vectors: [[5.6402, 0., 0.], [0., 5.6402, 0.], [0., 0., 5.6402]],
        crossing: vec![],
    };
    for a in &atoms {
        for b in atoms.iter().filter(|b| b.id > a.id) {
            if (length(sub(a.xyz, b.xyz)) - 2.8201).abs() > 1e-4 {
                continue;
            }
            doc.add_bond(a.id, b.id, 1, "plain");
            let axis = (0..3).find(|&k| (a.xyz[k] - b.xyz[k]).abs() > 1.).unwrap();
            let mut image = [0; 3];
            image[axis] = if b.xyz[axis] > a.xyz[axis] { -1 } else { 1 };
            cell.crossing.push(Crossing {
                a: a.id,
                b: b.id,
                image,
            });
        }
    }
    let mut model = Model3d::new(&doc, atoms, vec![], ForceField::default()).with_cell(
        &doc,
        Some(cell),
        vec![],
    );
    model.display = Display::ThreeD;
    model.perspective = 0.;
    doc.models.push(model);
    require_version(&mut doc);
    doc
}

#[test]
fn copy_opacity_round_trips_without_changing_default_files_or_versions() {
    let mut doc = salt();
    crystal::mark_distance_bonds(&mut doc);
    let version = doc.file_version();
    assert_eq!(doc.models[0].copy_opacity, 0.55);
    for opacity in [0.55, 0.25, 0., 1.] {
        doc.models[0].copy_opacity = opacity;
        let saved = doc.file_json().unwrap();
        let value: serde_json::Value = serde_json::from_slice(&saved).unwrap();
        assert_eq!(
            value["models"][0].get("copy_opacity").is_none(),
            opacity == 0.55
        );
        let reopened = Document::from_json(&saved).unwrap();
        assert_eq!(reopened.models, doc.models);
        assert_eq!(reopened.file_json().unwrap(), saved);
        assert_eq!(doc.file_version(), version);
    }
    for opacity in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
        doc.models[0].copy_opacity = opacity;
        assert!(validate(&doc).is_err(), "{opacity}");
    }
}

#[test]
fn boundary_and_repeat_copies_use_the_models_opacity_in_2d_and_3d() {
    let mut doc = salt();
    let alphas = |parts: &[Primitive]| {
        parts
            .iter()
            .filter_map(|p| match p {
                Primitive::Opacity { alpha, .. } => Some(*alpha),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    for repeat in [[1; 3], [2, 1, 1]] {
        doc.models[0].repeat = repeat;
        // Revisit an earlier opacity with otherwise identical drawings to
        // exercise the 2D copies cache as well as changes to the setting.
        for opacity in [0.55, 0.25, 0.8, 0.25, 0., 1.] {
            doc.models[0].copy_opacity = opacity;
            doc.models[0].display = Display::ThreeD;
            for style in [
                Style::BallStick,
                Style::Stick,
                Style::Wireframe,
                Style::SpaceFilling,
                Style::Ellipsoids,
            ] {
                doc.models[0].style = style;
                let faded = alphas(&primitives(&doc));
                // Opaque copies need no opacity; invisible ones are not drawn.
                assert_eq!(
                    faded.is_empty(),
                    opacity == 1. || opacity == 0.,
                    "{style:?}"
                );
                assert!(faded.iter().all(|&alpha| alpha == opacity), "{style:?}");
            }
            doc.models[0].display = Display::TwoD;
            let flat = flat_cells(&doc.models, &doc);
            let behind = alphas(&flat.behind);
            assert!(!behind.is_empty());
            assert!(behind.iter().all(|&alpha| alpha == opacity));
            let labels = alphas(&flat.front);
            assert_eq!(labels.is_empty(), opacity == 0.);
            assert!(labels.iter().all(|&alpha| alpha == opacity));
            if opacity == 0. {
                assert!(
                    !flat
                        .front
                        .iter()
                        .any(|p| matches!(p, Primitive::Halo { .. }))
                );
            }
        }
    }
}

#[test]
fn opaque_copies_cut_the_space_filling_balls_they_meet() {
    let mut doc = salt();
    doc.models[0].style = Style::SpaceFilling;
    // The balls not wrapped in an opacity: at 55% only the originals.
    let solid = |doc: &Document| {
        primitives(doc)
            .into_iter()
            .filter_map(|p| match p {
                Primitive::Sphere {
                    center, contact, ..
                } => Some((center, contact)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let originals = solid(&doc);
    assert_eq!(originals.len(), 8);
    doc.models[0].copy_opacity = 1.;
    let opaque = solid(&doc);
    assert_eq!(opaque.len(), 27);
    // The copies in front now cut the originals they meet.
    assert!(originals.iter().any(|ball| {
        opaque
            .iter()
            .all(|other| other.0 != ball.0 || other.1 != ball.1)
    }));
}

#[test]
fn nacl_3d_sites_and_bonds_match_the_2d_rule_for_the_whole_repeat() {
    let mut doc = salt();
    for (repeat, count, bonds) in [([1; 3], 27, 54), ([2; 3], 125, 300)] {
        doc.models[0].repeat = repeat;
        let model = &doc.models[0];
        let view = View::new(&doc, model).unwrap();
        let sites = shown_sites(&doc, model);
        assert_eq!(view.balls().count(), count);
        assert_eq!(view.originals, 8);
        assert_eq!(view.bonds.len(), bonds);
        assert_eq!(view.copies.len(), sites.len());
        for (id, image) in sites {
            let atom = model.atoms.iter().find(|a| a.id == id).unwrap();
            let expected = rotate(
                model.rotation,
                sub(
                    zip3(
                        atom.xyz,
                        model.cell.as_ref().unwrap().at(image.map(|v| v as f32)),
                        |p, t| p + t,
                    ),
                    view.center,
                ),
            );
            assert!(length(sub(view.rotated[view.copies[&(id, image)]], expected)) < 1e-5);
        }
        // Only the exterior of the shown box has dangling half-bonds.
        let whole = model.whole_cell().unwrap();
        for (i, end) in crossing_halves(&view, model) {
            let start = view.rotated[i];
            let inverse = [
                model.rotation[0],
                -model.rotation[1],
                -model.rotation[2],
                -model.rotation[3],
            ];
            let far = zip3(
                rotate(inverse, zip3(start, end, |a, b| 2. * b - a)),
                view.center,
                |p, c| p + c,
            );
            let f = whole.fractional(far);
            assert!(f.iter().any(|&v| !(-1e-4..=1.0001).contains(&v)), "{f:?}");
        }
        assert_eq!(crystal::composition(&doc, &[]).unwrap().formula, "Cl4Na4");
    }
}

#[test]
fn perovskite_has_titanium_on_eight_corners_and_oxygen_on_twelve_edges() {
    let mut doc = Document::default();
    let atoms: Vec<_> = [
        ("Ti", [0.; 3]),
        ("Sr", [0.5; 3]),
        ("O", [0.5, 0., 0.]),
        ("O", [0., 0.5, 0.]),
        ("O", [0., 0., 0.5]),
    ]
    .into_iter()
    .map(|(symbol, p)| {
        let id = doc.add_atom(symbol, Point::new(p[0] * 40., p[1] * 40.));
        Placed {
            id,
            xyz: p.map(|v| v * 3.899),
        }
    })
    .collect();
    let mut cell = Cell {
        vectors: [[3.899, 0., 0.], [0., 3.899, 0.], [0., 0., 3.899]],
        crossing: vec![],
    };
    for (axis, oxygen) in atoms[2..].iter().enumerate() {
        doc.add_bond(atoms[0].id, oxygen.id, 1, "plain");
        let mut image = [0; 3];
        image[axis] = -1;
        cell.crossing.push(Crossing {
            a: atoms[0].id,
            b: oxygen.id,
            image,
        });
    }
    let model = Model3d::new(&doc, atoms, vec![], ForceField::default()).with_cell(
        &doc,
        Some(cell),
        vec![],
    );
    let view = View::new(&doc, &model).unwrap();
    assert_eq!(view.symbols.iter().filter(|&&s| s == "Ti").count(), 8);
    assert_eq!(view.symbols.iter().filter(|&&s| s == "O").count(), 12);
    assert_eq!(view.symbols.iter().filter(|&&s| s == "Sr").count(), 1);
    assert_eq!(view.bonds.len(), 24);
    assert_eq!(model.atoms.len(), 5);
}

#[test]
fn space_filling_cell_ink_is_depth_tested_and_other_styles_keep_the_overlay() {
    let mut doc = salt();
    // Look down [101]: the face-centre Na lies in front of the origin Na.
    let angle = -std::f32::consts::FRAC_PI_8;
    doc.models[0].rotation = [angle.cos(), 0., angle.sin(), 0.];
    for perspective in [0., 0.7] {
        for theme in [
            crate::canvas_theme::CanvasTheme::Light,
            crate::canvas_theme::CanvasTheme::Dark,
        ] {
            doc.canvas_theme = theme;
            doc.models[0].perspective = perspective;
            for style in [
                Style::SpaceFilling,
                Style::BallStick,
                Style::Stick,
                Style::Wireframe,
            ] {
                doc.models[0].style = style;
                let model = &doc.models[0];
                let view = View::new(&doc, model).unwrap();
                let place = |p| rotate(model.rotation, sub(p, view.center));
                let project = |q| view.project(q).0;
                let mut frame = CellView {
                    place: &place,
                    project: &project,
                    camera: (perspective > 0.).then_some(view.camera),
                    width: (model.size / 220.).clamp(1., 3.),
                    size: (model.size * 0.06).clamp(8., 60.),
                    crowded: [false; 3],
                    occluders: None,
                };
                let whole = model.whole_cell().unwrap();
                let ink = float(theme.color([0; 3]));
                let paper = float(theme.background());
                let (_, old_edges, old_marks) = cell_ink(&frame, &whole, ink, paper);
                frame.occluders = (style == Style::SpaceFilling).then_some(&view);
                let (_, edges, marks) = cell_ink(&frame, &whole, ink, paper);
                let origin = project(place([0.; 3]));
                let dot = |parts: &[Primitive]| {
                    parts.iter().any(|p| matches!(p, Primitive::Sphere { center, focal, .. } if *center == origin && center == focal))
                };
                assert!(dot(&old_edges));
                let svg = |parts: &[Primitive]| {
                    scene::svg_part(parts, Point::new(-1000., -1000.), Point::new(1000., 1000.))
                };
                assert_eq!(svg(&marks), svg(&old_marks));
                if style == Style::SpaceFilling {
                    assert!(cell_occlusion::hidden(&view, place([0.; 3]), true));
                    assert!(!dot(&edges));
                    assert!(
                        !dot(&primitives(&doc)),
                        "origin dot must not reach the scene"
                    );
                    // Every cell edge passes through touching Na/Cl balls.
                    assert!(edges.is_empty(), "{perspective} {theme:?}: {edges:?}");
                } else {
                    assert_eq!(svg(&edges), svg(&old_edges), "{style:?}");
                    assert!(dot(&primitives(&doc)));
                }
            }
        }
    }
}

#[test]
fn translucent_copies_leave_the_original_surface_and_contacts_cache_intact() {
    let mut doc = salt();
    doc.models[0].style = Style::SpaceFilling;
    let mut view = View::new(&doc, &doc.models[0]).unwrap();
    let copy = view.originals;
    view.shown.fill(false);
    view.shown[0] = true;
    view.shown[copy] = true;
    view.symbols[0] = "C";
    view.symbols[copy] = "C";
    view.rotated[0] = [0.; 3];
    view.rotated[copy] = [0., 0., 0.5];
    let atoms = [0, copy];
    assert!(!view.contacts(&atoms, 1.).contains_key(&0));
    // The exact same pose must recalculate contacts when the front ball is opaque.
    view.solid = view.symbols.len();
    assert!(view.contacts(&atoms, 1.).contains_key(&0));
}

#[test]
fn hidden_hydrogens_do_not_leave_stubs_from_visible_copies() {
    let mut doc = salt();
    let hydrogen = doc.models[0].atoms[1].id;
    doc.atom_mut(hydrogen).unwrap().element = "H".into();
    doc.models[0].show_hydrogens = Hydrogens::None;
    let crossings = doc.models[0].cell.as_mut().unwrap();
    crossings
        .crossing
        .retain(|c| c.a == hydrogen || c.b == hydrogen);
    for style in [Style::Stick, Style::Wireframe] {
        doc.models[0].style = style;
        let view = View::new(&doc, &doc.models[0]).unwrap();
        assert!(crossing_halves(&view, &doc.models[0]).is_empty());
    }
}

#[test]
fn boundary_copies_fade_in_with_the_crystal_cell() {
    let mut doc = salt();
    for style in [Style::BallStick, Style::Stick, Style::Ellipsoids] {
        doc.models[0].style = style;
        doc.models[0].morph = Some(0.);
        let parts = primitives(&doc);
        assert!(parts.iter().any(|p| {
            let Primitive::Opacity { alpha, primitive } = p else {
                return false;
            };
            if *alpha != 0. {
                return false;
            }
            match primitive.as_ref() {
                Primitive::Sphere { center, focal, .. } => center != focal,
                Primitive::ShadedPolygon { .. }
                | Primitive::ShadedQuad { .. }
                | Primitive::Ellipsoid { .. } => true,
                _ => false,
            }
        }));
    }
}
