use super::*;
use crate::{
    arrows::{ArrowStyle, Preset},
    document::{Arrow, CALCULATIONS},
};
use std::collections::HashMap;

fn geometry() -> EnergyProfileGeometry {
    EnergyProfileGeometry {
        frame: Frame {
            numbers: vec![6, 8],
            positions_angstrom: vec![[0., 0., 0.], [1.4, 0.3, 0.2]],
            isotopes: vec![],
            masses_amu: vec![],
            charge: 0,
            multiplicity: 1,
            drawing_atoms: vec![],
        },
        bonds: vec![[0, 1]],
        forming: vec![],
        breaking: vec![],
    }
}
fn profile(energies: &[f64]) -> EnergyProfile {
    let states = energies
        .iter()
        .enumerate()
        .map(|(i, &energy_kcal)| EnergyProfileState {
            kind: if i == 0 {
                EnergyProfileStateKind::Reactant
            } else if i + 1 == energies.len() {
                EnergyProfileStateKind::Product
            } else if i % 2 == 1 {
                EnergyProfileStateKind::TransitionState
            } else {
                EnergyProfileStateKind::Intermediate
            },
            label: if i == 0 {
                "R".into()
            } else if i + 1 == energies.len() {
                "P".into()
            } else if i % 2 == 1 {
                format!("TS{}‡", i.div_ceil(2))
            } else {
                "I".into()
            },
            energy_kcal,
            geometry: Some(geometry()),
        })
        .collect();
    let steps = (0..energies.len().saturating_sub(1))
        .step_by(2)
        .map(|from| EnergyProfileStep {
            from,
            to: from + 2,
            transition: Some(from + 1),
            irc: vec![],
        })
        .collect();
    EnergyProfile::new(1, "a".repeat(64), "GFN2-xTB".into(), states, steps)
}
fn placed(profile: EnergyProfile) -> Graphic {
    let (w, h) = profile.natural_size(&style::DEFAULT);
    Graphic::energy_profile(
        2,
        Point::new(10., 20.),
        Point::new(w, h),
        profile,
        GraphicStyle::default(),
    )
}
fn overlaps((a, b): (Point, Point), (c, d): (Point, Point)) -> bool {
    a.x < d.x && b.x > c.x && a.y < d.y && b.y > c.y
}

#[test]
fn three_and_five_states_keep_labels_and_models_clear_of_every_curve() {
    for energies in [
        &[0., 18., -8.][..],
        &[0., 18., -5., 14., -12.],
        &[0., 0.00003, 0.00001, 0.00004, 0.00002],
        &[0., -4., -6., -5., -20.],
        &[0., 18., 12.],
    ] {
        let p = profile(energies);
        let g = placed(p.clone());
        let layout = Layout::new(&p, &style::DEFAULT);
        assert_eq!(layout.states.len(), energies.len());
        let paths: Vec<_> = layout
            .curves
            .iter()
            .flat_map(|p| super::super::flattened(p))
            .collect();
        for label in layout.labels.iter().rev().take(p.states.len() * 2) {
            let a = layout.annotation(&g, label);
            let (w, h) = a.size();
            let bounds = (
                label.centre.offset(-w / 2., -h / 2.),
                label.centre.offset(w / 2., h / 2.),
            );
            for path in &paths {
                for segment in path.windows(2) {
                    let [a, b] = segment else { continue };
                    // All curves live horizontally outside the column occupied by
                    // this state's text. Thus their bounding boxes cannot meet it.
                    let line = (
                        Point::new(a.x.min(b.x), a.y.min(b.y) - 0.01),
                        Point::new(a.x.max(b.x), a.y.max(b.y) + 0.01),
                    );
                    assert!(
                        !overlaps(bounds, line),
                        "{} intersects curve for {energies:?}",
                        label.text
                    );
                }
            }
            for model in &layout.pictures {
                let half = layout.picture_side / 2.;
                assert!(!overlaps(
                    bounds,
                    (
                        model.centre.offset(-half, -half),
                        model.centre.offset(half, half)
                    )
                ));
            }
        }
        assert!(
            layout
                .states
                .windows(2)
                .all(|s| s[0].point.x + layout.bar / 2. < s[1].point.x - layout.bar / 2.)
        );
    }
}

#[test]
fn model_placement_uses_the_free_side_or_a_shared_top_row() {
    let mut p = profile(&[0., 18., -6., 14., -10.]);
    let levels = Layout::new(&p, &style::DEFAULT);
    assert_eq!(levels.pictures.len(), 5);
    for model in &levels.pictures {
        let state = &levels.states[model.state];
        if p.states[model.state].kind == EnergyProfileStateKind::TransitionState {
            assert!(model.centre.y + levels.picture_side / 2. < state.point.y);
        } else {
            assert!(model.centre.y - levels.picture_side / 2. > state.point.y);
        }
    }
    p.models = EnergyProfileModels::RowAbove;
    let row = Layout::new(&p, &style::DEFAULT);
    assert!(
        row.pictures
            .iter()
            .all(|m| m.centre.y == row.pictures[0].centre.y
                && m.centre.y + row.picture_side / 2. < row.top)
    );
    p.models = EnergyProfileModels::Off;
    let off = Layout::new(&p, &style::DEFAULT);
    assert!(off.pictures.is_empty());
    assert!(off.height < levels.height && off.height < row.height);
}

#[test]
fn axes_round_in_the_display_unit_and_flat_or_negative_profiles_remain_finite() {
    for unit in EnergyUnit::ALL {
        for energies in [&[0., 18.37, -7.8][..], &[0., 0., 0.], &[-10., -6., -20.]] {
            let mut p = profile(energies);
            p.unit = unit;
            let layout = Layout::new(&p, &style::DEFAULT);
            let factor = unit.convert_kcal(1.);
            let (_, _, ticks) = nice_scale(energies.iter().map(|e| e * factor));
            let step = ticks[1] - ticks[0];
            let magnitude = 10_f64.powf(step.log10().floor());
            let fraction = step / magnitude;
            assert!(
                [1., 2., 5., 10.]
                    .into_iter()
                    .any(|v| (fraction - v).abs() < 1e-8)
            );
            for tick in ticks {
                assert!(layout.labels.iter().any(|l| l.text == tick_text(tick)));
            }
            assert!(layout.states.iter().all(|s| s.point.is_finite()));
            assert!(
                layout
                    .labels
                    .iter()
                    .any(|l| l.text == format!("ΔE ({}) · GFN2-xTB", unit.label()))
            );
        }
    }
}

#[test]
fn owned_geometries_render_as_vector_models_with_thematic_bond_changes() {
    let mut p = profile(&[0., 18., -6.]);
    let g3 = p.states[1].geometry.as_mut().unwrap();
    g3.forming = vec![[0, 1]];
    g3.breaking = vec![[0, 1]];
    let g = placed(p);
    let doc = Document {
        graphics: vec![g.clone()],
        ..Document::default()
    };
    let (parts, labels, pictures) = g.energy_profile_scene(&doc.drawing_style, &doc);
    assert!(!parts.is_empty() && !labels.is_empty());
    assert_eq!(
        pictures
            .iter()
            .filter(|p| matches!(p, Primitive::Sphere { .. }))
            .count(),
        6
    );
    assert!(pictures.iter().any(|p|matches!(p,Primitive::Path{style,..} if style.stroke==Color::Palette(Hue::Green,Row::Strong))));
    assert!(pictures.iter().any(|p|matches!(p,Primitive::Path{style,..} if style.stroke==Color::Palette(Hue::Red,Row::Strong) && style.pattern==LinePattern::Solid)));
    let all = crate::scene::primitives(&doc);
    assert!(all.iter().any(|p| matches!(p, Primitive::Sphere { .. })));
    let svg = crate::scene::svg(&doc);
    assert!(!svg.contains("<image"));
    assert!(svg.contains("GFN2-xTB"));
}

#[test]
fn irregular_irc_samples_keep_coordinates_and_exact_state_anchors() {
    let mut p = profile(&[0., 20., -8.]);
    p.steps[0].irc = vec![
        EnergyProfileSample {
            x: 0.,
            energy_kcal: 9.,
        },
        EnergyProfileSample {
            x: 0.07,
            energy_kcal: 4.,
        },
        EnergyProfileSample {
            x: 0.5,
            energy_kcal: 19.,
        },
        EnergyProfileSample {
            x: 0.84,
            energy_kcal: -1.,
        },
        EnergyProfileSample {
            x: 1.,
            energy_kcal: -7.,
        },
    ];
    let layout = Layout::new(&p, &style::DEFAULT);
    let path = &layout.curves[0];
    let pts: Vec<_> = path.iter().flat_map(PathCommand::iter_points).collect();
    let r = &layout.states[0];
    let ts = &layout.states[1];
    let end = &layout.states[2];
    assert!(pts.contains(&r.point.offset(layout.bar / 2., 0.)));
    assert!(pts.contains(&ts.point.offset(-layout.bar / 2., 0.)));
    assert!(pts.contains(&ts.point.offset(layout.bar / 2., 0.)));
    assert!(pts.contains(&end.point.offset(-layout.bar / 2., 0.)));
    let expected = r.point.x + layout.bar / 2. + (ts.point.x - r.point.x - layout.bar) * 0.14;
    assert!(
        pts.iter()
            .any(|q| (q.x - expected).abs() < 1e-4 && (q.y - layout.energy_y(4.)).abs() < 1e-4)
    );
}

#[test]
fn affine_frame_keeps_figure_selection_and_labels() {
    let p = profile(&[0., 18., -6.]);
    let mut g = placed(p.clone());
    let origin = g.origin;
    g.map_positions(|p| p.turned(origin, 35.));
    let layout = Layout::new(&p, &style::DEFAULT);
    for s in &layout.states {
        assert!(g.hit(layout.world(&g, s.point), 0.));
    }
    for label in layout.labels.iter().rev().take(p.states.len() * 2) {
        assert_eq!(layout.annotation(&g, label).angle, 35.);
    }
    assert!(g.edit_points().is_empty());
    let before = g.clone();
    g.edit_point(0, Point::new(0., 0.)).unwrap();
    assert_eq!(g, before);
    let (lo, hi) = g.shape_bounds();
    assert!(
        g.frame_corners()
            .into_iter()
            .all(|p| (lo.x..=hi.x).contains(&p.x) && (lo.y..=hi.y).contains(&p.y))
    );
}

#[test]
fn rerun_retains_frame_style_and_molecular_view() {
    let mut g = placed(profile(&[0., 18., -6.]));
    let frame = (g.origin, g.axis_x, g.axis_y);
    let style = g.style.clone();
    let p = g.energy_profile.as_mut().unwrap();
    p.models = EnergyProfileModels::RowAbove;
    p.rotation = [0., 1., 0., 0.];
    let mut fresh = profile(&[0., 23., -5., 18., -10.]);
    fresh.unit = EnergyUnit::KjPerMol;
    p.relink(fresh);
    assert_eq!(p.models, EnergyProfileModels::RowAbove);
    assert_eq!(p.rotation, [0., 1., 0., 0.]);
    assert_eq!(p.states.len(), 5);
    assert_eq!(p.unit, EnergyUnit::KjPerMol);
    assert_eq!((g.origin, g.axis_x, g.axis_y), frame);
    assert_eq!(g.style, style);
}

#[test]
fn native_roundtrip_budget_and_validation_cover_profile_payloads() {
    let g = placed(profile(&[0., 18., -6.]));
    let doc = Document {
        graphics: vec![g.clone()],
        ..Document::default()
    };
    assert_eq!(doc.file_version(), CALCULATIONS);
    doc.validate().unwrap();
    let opened = Document::from_json(&serde_json::to_vec(&doc.current()).unwrap()).unwrap();
    assert_eq!(opened.graphics[0], g);
    let variants: [fn(&mut EnergyProfile); 9] = [
        |p| p.states.clear(),
        |p| p.states[0].energy_kcal = f64::INFINITY,
        |p| p.source_key = "wrong".into(),
        |p| p.steps[0].to = 0,
        |p| p.steps[0].transition = None,
        |p| p.rotation = [0.; 4],
        |p| p.states[0].geometry.as_mut().unwrap().frame.drawing_atoms = vec![Some(1), Some(2)],
        |p| p.states[0].geometry.as_mut().unwrap().bonds.push([0, 9]),
        |p| {
            p.steps[0].irc = vec![
                EnergyProfileSample {
                    x: 0.8,
                    energy_kcal: 3.,
                },
                EnergyProfileSample {
                    x: 0.2,
                    energy_kcal: 4.,
                },
            ]
        },
    ];
    for change in variants {
        let mut invalid = g.clone();
        change(invalid.energy_profile.as_mut().unwrap());
        assert!(invalid.validate().is_err());
    }
    let mut wrong = g.clone();
    wrong.energy_profile = None;
    assert!(wrong.validate().is_err());
    let mut wrong = g;
    wrong.axis_y = wrong.axis_x;
    assert!(wrong.validate().is_err());
}

#[test]
fn remap_preserves_source_and_provenance_and_deletion_leaves_stale_values() {
    let g = placed(profile(&[0., 18., -6.]));
    let arrow = Arrow::new(
        1,
        Point::default(),
        Point::new(50., 0.),
        Preset::Forward,
        ArrowStyle::default(),
    );
    let mut doc = Document {
        graphics: vec![g],
        arrows: vec![arrow],
        ..Document::default()
    };
    crate::editing::remap_ids(&mut doc, &HashMap::from([(1, 10), (2, 20)])).unwrap();
    let p = doc.graphics[0].energy_profile.as_ref().unwrap();
    assert_eq!(p.source, Some(10));
    assert!(p.linked);
    assert_eq!(p.source_key, "a".repeat(64));
    let states = p.states.clone();
    doc.delete(&[10]);
    let p = doc.graphics[0].energy_profile.as_ref().unwrap();
    assert_eq!(p.source, None);
    assert!(p.linked);
    assert_eq!(p.states, states);
    doc.validate().unwrap();
    crate::editing::remap_ids(&mut doc, &HashMap::from([(20, 200)])).unwrap();
    assert!(doc.graphics[0].energy_profile.as_ref().unwrap().linked);
    assert_eq!(
        doc.graphics[0].energy_profile.as_ref().unwrap().source_key,
        "a".repeat(64)
    );
    doc.graphics[0].energy_profile.as_mut().unwrap().unlink();
    assert!(!doc.graphics[0].energy_profile.as_ref().unwrap().linked);
}
