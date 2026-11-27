use super::*;
use crate::{
    arrows::{ArrowStyle, Preset},
    document::{Arrow, ENERGY_PROFILES},
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
        .map(|from| {
            let to = (from + 2).min(energies.len() - 1);
            EnergyProfileStep {
                from,
                to,
                transition: (to == from + 2).then_some(from + 1),
                source_key: None,
                irc: vec![],
            }
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
fn contains((a, b): (Point, Point), (c, d): (Point, Point)) -> bool {
    a.x <= c.x && b.x >= d.x && a.y <= c.y && b.y >= d.y
}

fn assert_frame_contains(g: &Graphic, parts: &[Part], labels: &[Annotation], models: &[Primitive]) {
    let det = g.frame_determinant();
    let x = g.axis_y.x.hypot(g.axis_y.y) / det.abs();
    let y = g.axis_x.x.hypot(g.axis_x.y) / det.abs();
    let check = |p, radius, kind: &str| {
        let p = g.world_to_local(p, det);
        let eps = 1e-5;
        assert!(
            (x * radius - eps..=1. - x * radius + eps).contains(&p.x)
                && (y * radius - eps..=1. - y * radius + eps).contains(&p.y),
            "{kind} outside the frame at {p:?} with radius {radius}"
        );
    };
    for part in parts {
        for p in part.commands.iter().flat_map(PathCommand::iter_points) {
            check(p, part.style.width() / 2., "plot stroke");
        }
    }
    for label in labels {
        for corner in label.corners() {
            check(corner, 0., &label.text);
        }
    }
    for model in models {
        assert_model_inside(model, &check);
    }
}

fn assert_model_inside(model: &Primitive, check: &impl Fn(Point, f32, &str)) {
    match model {
        Primitive::Opacity { primitive, .. } => assert_model_inside(primitive, check),
        Primitive::Path {
            commands, style, ..
        } => {
            for p in commands.iter().flat_map(PathCommand::iter_points) {
                check(p, style.width() / 2., "model path");
            }
        }
        Primitive::Sphere {
            center,
            radius,
            outline,
            contact,
            ..
        } => {
            let half = outline.map_or(0., |outline| outline.width / 2.);
            check(*center, *radius + half, "atom");
            if let Some(contact) = contact {
                for p in contact.region.iter().flatten() {
                    check(*p, half, "atom outline");
                }
            }
        }
        Primitive::ShadedQuad {
            corners,
            cap,
            outline,
            ..
        } => {
            let half = outline.map_or(0., |outline| outline.width / 2.);
            for p in corners.iter().chain(cap) {
                check(*p, half, "bond");
            }
        }
        other => panic!("unexpected molecular artwork: {other:?}"),
    }
}

fn assert_no_overlaps(g: &Graphic, layout: &Layout, parts: &[Part], labels: &[Annotation]) {
    let frame = g.shape_bounds();
    let pictures: Vec<_> = layout
        .pictures
        .iter()
        .map(|m| {
            let (lo, hi) = m.bounds();
            (layout.world(g, lo), layout.world(g, hi))
        })
        .collect();
    let labels: Vec<_> = labels.iter().map(|l| (&l.text, l.bounds())).collect();
    for (i, &(text, bounds)) in labels.iter().enumerate() {
        assert!(contains(frame, bounds), "{text} outside frame");
        for &(other, other_bounds) in &labels[i + 1..] {
            assert!(!overlaps(bounds, other_bounds), "{text} and {other}");
        }
        assert!(
            pictures.iter().all(|&model| !overlaps(bounds, model)),
            "{text} and model"
        );
    }
    for (i, &model) in pictures.iter().enumerate() {
        assert!(contains(frame, model));
        assert!(
            pictures[i + 1..]
                .iter()
                .all(|&other| !overlaps(model, other))
        );
    }
    for part in parts {
        let half = part.style.width() / 2.;
        for path in super::super::flattened(&part.commands) {
            for pair in path.windows(2) {
                let hits = |(lo, hi): (Point, Point)| {
                    crate::atom_labels::segment_hits_rect(
                        pair[0],
                        pair[1],
                        lo.offset(-half, -half),
                        hi.offset(half, half),
                    )
                };
                for &(text, bounds) in &labels {
                    assert!(!hits(bounds), "{text} and stroke");
                }
                assert!(
                    pictures.iter().all(|&model| !hits(model)),
                    "model and stroke"
                );
            }
        }
    }
}

#[test]
fn transition_state_labels_only_use_numbers_for_multiple_transitions() {
    // The fixture uses the numbered default names found in older saved profiles.
    for (energies, expected) in [
        (&[0., 27.3, -7.6][..], &["TS‡ 27.3"][..]),
        (&[0., 27.3, -7.6, -4.], &["TS‡ 27.3"]),
        (&[0., 27.3, -7.6, 18., -4.], &["TS1‡ 27.3", "TS2‡ 18.0"]),
    ] {
        let (_, labels) = placed(profile(energies)).energy_profile_parts(&style::DEFAULT);
        let transitions: Vec<_> = labels
            .iter()
            .filter(|l| l.text.starts_with("TS"))
            .map(|l| l.text.as_str())
            .collect();
        assert_eq!(transitions, expected);
    }
}

#[test]
fn default_profiles_keep_the_caption_and_models_close_to_their_anchors() {
    for (energies, reactant, product, ts) in [
        (
            [0., 27.3, -7.6],
            "3-(ethenyloxy)prop-1-ene",
            "pent-4-enal",
            "TS‡ 27.3",
        ),
        (
            [0., 7., -67.8],
            "buta-1,3-diene + ethene",
            "cyclohexene",
            "TS‡ 7.0",
        ),
    ] {
        let mut p = profile(&energies);
        p.states[0].label = reactant.into();
        p.states[2].label = product.into();
        let layout = Layout::new(&p, &style::DEFAULT);
        let g = placed(p.clone());
        let (parts, labels, models) = g.energy_profile_scene(&style::DEFAULT, &Document::default());
        assert_no_overlaps(&g, &layout, &parts, &labels);
        assert_frame_contains(&g, &parts, &labels, &models);
        let em = layout.font.size();
        let axis = &layout.lines[0];
        let plot = (
            layout.world(&g, axis[0]),
            layout.world(&g, Point::new(layout.width - em * 0.4, axis[1].y)),
        );
        let caption = labels.iter().find(|l| l.text == p.method).unwrap().bounds();
        assert!(contains(plot, caption));
        assert!(caption.0.x - plot.0.x < em * 0.5 && caption.0.y - plot.0.y < em * 0.5);

        assert_eq!(layout.pictures.len(), 3);
        let mut remaining = models.as_slice();
        for (slot, (text, above)) in
            layout
                .pictures
                .iter()
                .zip([(reactant, false), (ts, true), (product, false)])
        {
            let (artwork, rest) = remaining.split_at(slot.artwork.len());
            remaining = rest;
            let (lo, hi) = crate::scene::bounds(artwork);
            let (a, b) = labels.iter().find(|l| l.text == text).unwrap().bounds();
            let padding = style::DEFAULT.world(4.);
            let gap = if above {
                a.y - (hi.y - padding)
            } else {
                lo.y + padding - b.y
            };
            assert!((0. ..em * 0.4).contains(&gap), "{text} model gap: {gap}");
            assert!(((lo.x + hi.x) / 2. - (a.x + b.x) / 2.).abs() < 1e-3);
        }
        assert!(remaining.is_empty());
        let (lo, hi) = g.shape_bounds();
        let (model_lo, model_hi) = crate::scene::bounds(&models);
        let padding = style::DEFAULT.world(4.);
        assert!(model_lo.y + padding - lo.y < em * 0.4);
        assert!(hi.y - (model_hi.y - padding) < em * 0.4);
    }
}

#[test]
fn blocked_method_caption_moves_to_the_top_right_inside_the_plot() {
    let mut p = profile(&[0., -60.]);
    p.models = EnergyProfileModels::Off;
    p.states[0].label = "Reactant".into();
    for method in ["GFN2-xTB + g-xTB".into(), "W".repeat(128)] {
        p.method = method;
        let layout = Layout::new(&p, &style::DEFAULT);
        let g = placed(p.clone());
        let (parts, labels) = g.energy_profile_parts(&style::DEFAULT);
        assert_no_overlaps(&g, &layout, &parts, &labels);
        let em = layout.font.size();
        let axis = &layout.lines[0];
        let plot = (
            layout.world(&g, axis[0]),
            layout.world(&g, Point::new(layout.width - em * 0.4, axis[1].y)),
        );
        let caption = labels.iter().find(|l| l.text == p.method).unwrap().bounds();
        assert!(contains(plot, caption));
        assert!(plot.1.x - caption.1.x < em * 0.5 && caption.0.y - plot.0.y < em * 0.5);
    }
    let mut compact = placed(p);
    compact.axis_x = Point::new(10., 0.);
    compact.style.width_pt = 12.;
    let (parts, labels) = compact.energy_profile_parts(&style::DEFAULT);
    assert_frame_contains(&compact, &parts, &labels, &[]);
    assert!(parts.iter().all(|part| part.style.width_pt >= 0.1));
}

#[test]
fn axis_top_caption_clears_the_rendered_axis() {
    let mut p = profile(&[0., 27.3, -7.6]);
    p.method = "GFN2-xTB + g-xTB".into();
    for (width_pt, scale) in [(12., 1.), (0.6, 0.08)] {
        let mut g = placed(p.clone());
        g.style.width_pt = width_pt;
        let origin = g.origin;
        g.map_positions(|point| origin.lerp(point, scale));
        let (parts, labels, models) = g.energy_profile_scene(&style::DEFAULT, &Document::default());
        assert_frame_contains(&g, &parts, &labels, &models);
        let caption = labels.iter().find(|l| l.text == p.method).unwrap();
        let axis = &parts[0];
        let PathCommand::Move(axis_top) = axis.commands[0] else {
            panic!("expected the energy axis");
        };
        assert_eq!(axis.style.width_pt, width_pt);
        assert!(caption.bounds().0.y > axis_top.y);
        assert!(
            caption.bounds().0.x > axis_top.x + axis.style.width() / 2.,
            "caption intersects the axis at {width_pt} pt and scale {scale}"
        );
    }
}

#[test]
fn small_frames_contain_a_bar_at_the_bottom_of_the_plot() {
    let mut p = profile(&[0., 18., -20., 14., -5.]);
    for mode in EnergyProfileModels::ALL {
        p.models = mode;
        for (width_pt, scale) in [(0.6, 0.1), (12., 0.25)] {
            let mut g = placed(p.clone());
            g.style.width_pt = width_pt;
            let origin = g.origin;
            g.map_positions(|point| origin.lerp(point, scale));
            let (parts, labels, models) =
                g.energy_profile_scene(&style::DEFAULT, &Document::default());
            assert_eq!(models.is_empty(), mode == EnergyProfileModels::Off);
            assert_frame_contains(&g, &parts, &labels, &models);
            let bar = &parts[parts.len() - 3];
            let [PathCommand::Move(left), PathCommand::Line(_)] = bar.commands.as_slice() else {
                panic!("expected the central bar");
            };
            let PathCommand::Line(plot_bottom) = parts[0].commands[1] else {
                panic!("expected the bottom of the energy axis");
            };
            assert_eq!(left.y, plot_bottom.y);
        }
    }
}

#[test]
fn compressed_affine_frame_reduces_effective_strokes() {
    let mut p = profile(&[0., 18., -20., 14., -5.]);
    let geometry = p.states[1].geometry.as_mut().unwrap();
    geometry.forming = vec![[0, 1]];
    geometry.breaking = vec![[0, 1]];
    for mode in EnergyProfileModels::ALL {
        p.models = mode;
        let mut g = placed(p.clone());
        g.style.width_pt = 12.;
        let origin = g.origin;
        g.map_positions(|p| {
            let p = origin.lerp(p, 0.1).turned(origin, 45.);
            Point::new(p.x, origin.y + (p.y - origin.y) * 0.05)
        });
        g.validate().unwrap();
        let (parts, labels, models) = g.energy_profile_scene(&style::DEFAULT, &Document::default());
        assert_eq!(models.is_empty(), mode == EnergyProfileModels::Off);
        assert_frame_contains(&g, &parts, &labels, &models);
        assert!(
            parts
                .iter()
                .all(|p| (0.1..g.style.width_pt).contains(&p.style.width_pt))
        );
    }
}

#[test]
fn labels_and_models_stay_clear_of_each_other_and_every_curve() {
    for energies in [
        &[0., -60.][..],
        &[0., 18., -8.],
        &[0., 18., -5., 14., -12.],
        &[0., 18., -20., 14., -5.],
        &[0., 0.00003, 0.00001, 0.00004, 0.00002],
        &[0., -4., -6., -5., -20.],
        &[0., 18., 12.],
    ] {
        let mut p = profile(energies);
        p.states[0].label = "3-(ethenyloxy)prop-1-ene + cis-2-butene".into();
        if energies.len() > 2 {
            p.states[1].label = "Transition-state conformer".into();
            p.steps[0].irc = vec![
                EnergyProfileSample {
                    x: 0.01,
                    energy_kcal: -30.,
                },
                EnergyProfileSample {
                    x: 0.98,
                    energy_kcal: 40.,
                },
            ];
        }
        for mode in EnergyProfileModels::ALL {
            p.models = mode;
            let g = placed(p.clone());
            let layout = Layout::new(&p, &style::DEFAULT);
            let (parts, labels) = layout.ink(&g);
            assert_no_overlaps(&g, &layout, &parts, &labels);
        }
    }
}

#[test]
fn short_equal_bars_and_smooth_connectors_span_evenly_spaced_levels() {
    for energies in [&[0., -6.][..], &[0., 18., -6.], &[0., 18., -6., 12., -17.]] {
        let p = profile(energies);
        let g = placed(p.clone());
        let layout = Layout::new(&p, &style::DEFAULT);
        let (parts, _) = layout.ink(&g);
        let (other, bars) = parts.split_at(parts.len() - energies.len());
        let slot = layout.states[1].x - layout.states[0].x;
        assert!((0.10..=0.12).contains(&(layout.bar / (slot * energies.len() as f32))));
        assert!(
            other
                .iter()
                .all(|p| p.style.width_pt == style::DEFAULT.line_width_pt)
        );
        for bar in bars {
            let [PathCommand::Move(a), PathCommand::Line(b)] = bar.commands.as_slice() else {
                panic!("level should be horizontal");
            };
            assert!((b.x - a.x - layout.bar).abs() < 1e-3);
            assert_eq!(a.y, b.y);
            assert_eq!(bar.style.width_pt, 2. * style::DEFAULT.line_width_pt);
        }
        for (pair, gap) in layout.states.windows(2).zip(&layout.curves) {
            assert!((pair[1].x - pair[0].x - slot).abs() < 1e-3);
            let a = pair[0].offset(layout.bar / 2., 0.);
            let b = pair[1].offset(-layout.bar / 2., 0.);
            let [PathCommand::Move(start), PathCommand::Cubic(c1, c2, end)] = gap.as_slice() else {
                panic!("connector should be cubic");
            };
            assert_eq!((*start, *end), (a, b));
            assert_eq!((c1.y, c2.y), (a.y, b.y));
        }
    }
}

#[test]
fn model_placement_uses_the_free_side_or_a_shared_top_row() {
    let mut p = profile(&[0., 18., -6., 14., -10.]);
    let levels = Layout::new(&p, &style::DEFAULT);
    assert_eq!(levels.pictures.len(), 5);
    for ((model, state), datum) in levels.pictures.iter().zip(&levels.states).zip(&p.states) {
        let (lo, hi) = model.bounds();
        if datum.kind == EnergyProfileStateKind::TransitionState {
            assert!(hi.y < state.y);
        } else {
            assert!(lo.y > state.y);
        }
    }
    assert!((1. ..=1.5).contains(&(levels.picture_side / levels.bar)));
    let picture = geometry_picture(
        &geometry(),
        p.rotation,
        levels.picture_side,
        &Document::default(),
    )
    .unwrap();
    let (lo, hi) = crate::scene::bounds(&picture);
    assert!((1. ..=1.5).contains(&((hi.x - lo.x).max(hi.y - lo.y) / levels.bar)));
    p.models = EnergyProfileModels::RowAbove;
    let row = Layout::new(&p, &style::DEFAULT);
    assert_eq!(row.picture_side, levels.picture_side);
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
            let axis = &layout.lines[0];
            assert_eq!(axis[0], layout.lines.last().unwrap()[1]);
            assert_eq!(axis[1], layout.lines[1][1]);
            for (&tick, mark) in ticks.iter().zip(&layout.lines[1..]) {
                assert_eq!(mark[0].y, mark[1].y);
                assert_eq!(mark[1].x, axis[0].x);
                assert!(mark[0].x < mark[1].x);
                assert!(
                    layout
                        .labels
                        .iter()
                        .any(|l| l.text == tick_text(tick) && l.centre.y == mark[1].y)
                );
            }
            assert!(layout.states.iter().all(|s| s.is_finite()));
            let title = &layout.labels[0];
            let caption = layout.labels.last().unwrap();
            assert_eq!(title.text, format!("ΔE ({})", unit.label()));
            assert_eq!(title.angle, -90.);
            assert_eq!(caption.text, "GFN2-xTB");
            assert_eq!(caption.angle, 0.);
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
    assert!(labels.iter().any(|l| l.text == "TS‡ 18.0"));
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
fn irc_joins_are_continuous_and_tangent_at_exact_state_levels() {
    let mut p = profile(&[0., 20., -8.]);
    p.steps[0].irc = vec![
        EnergyProfileSample {
            x: 0.,
            energy_kcal: 9.,
        },
        EnergyProfileSample {
            x: 0.04,
            energy_kcal: 2.,
        },
        EnergyProfileSample {
            x: 0.1,
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
            x: 0.96,
            energy_kcal: -5.,
        },
        EnergyProfileSample {
            x: 1.,
            energy_kcal: -7.,
        },
    ];
    let layout = Layout::new(&p, &style::DEFAULT);
    for ((path, pair), (fraction, energy)) in layout
        .curves
        .iter()
        .zip(layout.states.windows(2))
        .zip([(0.2, 4.), (0.68, -1.)])
    {
        let [
            PathCommand::Move(start),
            PathCommand::Cubic(start_handle, enter_handle, enter),
            PathCommand::Line(sample),
            PathCommand::Line(leave),
            PathCommand::Cubic(leave_handle, end_handle, end),
        ] = path.as_slice()
        else {
            panic!("each gap should contain one continuous branch");
        };
        assert_eq!(*start, pair[0].offset(layout.bar / 2., 0.));
        assert_eq!(*end, pair[1].offset(-layout.bar / 2., 0.));
        assert_eq!(start_handle.y, start.y);
        assert_eq!(end_handle.y, end.y);
        assert!(start.x < start_handle.x && end_handle.x < end.x);
        for (point, t) in [(enter, 0.15), (sample, fraction), (leave, 0.85)] {
            assert!((point.x - start.lerp(*end, t).x).abs() < 1e-4);
        }
        assert_eq!(sample.y, layout.energy_y(energy));
        let slope = |a: Point, b: Point| {
            assert!(a.x < b.x);
            (b.y - a.y) / (b.x - a.x)
        };
        assert!((slope(*enter_handle, *enter) - slope(*enter, *sample)).abs() < 1e-4);
        assert!((slope(*sample, *leave) - slope(*leave, *leave_handle)).abs() < 1e-4);
    }
    assert_eq!(layout.states[1].y, layout.energy_y(20.));
}

#[test]
fn affine_frame_keeps_figure_selection_and_labels() {
    let p = profile(&[0., 18., -6.]);
    let mut g = placed(p.clone());
    let origin = g.origin;
    g.map_positions(|p| p.turned(origin, 35.));
    let layout = Layout::new(&p, &style::DEFAULT);
    for s in &layout.states {
        assert!(g.hit(layout.world(&g, *s), 0.));
    }
    for label in &layout.labels {
        let annotation = layout.annotation(&g, label);
        assert_eq!(
            annotation.angle,
            if label.text.starts_with("ΔE") {
                305.
            } else {
                35.
            }
        );
        assert!(annotation.centre().distance(layout.world(&g, label.centre)) < 1e-4);
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
    assert_eq!(doc.file_version(), ENERGY_PROFILES);
    doc.validate().unwrap();
    let opened = Document::from_json(&serde_json::to_vec(&doc.current()).unwrap()).unwrap();
    assert_eq!(opened.graphics[0], g);
    let variants: [fn(&mut EnergyProfile); 10] = [
        |p| p.states.clear(),
        |p| p.states[0].energy_kcal = f64::INFINITY,
        |p| p.source_key = "wrong".into(),
        |p| p.steps[0].source_key = Some("wrong".into()),
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

fn playback_marker(overlay: &[Primitive]) -> Point {
    let Primitive::Path {
        commands,
        filled: true,
        ..
    } = &overlay[1]
    else {
        panic!("Missing playback marker")
    };
    let points: Vec<_> = commands.iter().flat_map(PathCommand::iter_points).collect();
    let lo = points
        .iter()
        .fold(Point::new(f32::INFINITY, f32::INFINITY), |lo, p| {
            Point::new(lo.x.min(p.x), lo.y.min(p.y))
        });
    let hi = points
        .iter()
        .fold(Point::new(f32::NEG_INFINITY, f32::NEG_INFINITY), |hi, p| {
            Point::new(hi.x.max(p.x), hi.y.max(p.y))
        });
    Point::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.)
}

#[test]
fn playback_marker_follows_drawn_irc_blends_and_every_level_after_affine_placement() {
    let mut p = profile(&[0., 18., -6., 12., -17.]);
    for step in &mut p.steps {
        step.irc = vec![
            EnergyProfileSample {
                x: 0.01,
                energy_kcal: -9.,
            },
            EnergyProfileSample {
                x: 0.12,
                energy_kcal: 10.,
            },
            EnergyProfileSample {
                x: 0.4,
                energy_kcal: 16.,
            },
            EnergyProfileSample {
                x: 0.6,
                energy_kcal: 8.,
            },
            EnergyProfileSample {
                x: 0.9,
                energy_kcal: -12.,
            },
        ];
    }
    let mut g = placed(p.clone());
    g.style.width_pt = 12.;
    g.axis_x = Point::new(g.axis_x.x * 0.8, g.axis_x.x * 0.45);
    g.axis_y = Point::new(-g.axis_y.y * 0.1, g.axis_y.y * 0.5);
    let source = Document::default();
    let (parts, labels, _) = g.energy_profile_scene(&style::DEFAULT, &source);
    assert!(
        labels
            .iter()
            .all(|label| label.text != "Reaction coordinate")
    );
    let bars = &parts[parts.len() - p.states.len()..];
    let paths: Vec<_> = parts
        .iter()
        .rev()
        .take(p.states.len() + p.steps.len() * 2)
        .flat_map(|part| super::super::flattened(&part.commands))
        .collect();
    for (index, step) in p.steps.iter().enumerate() {
        for x in [0., 0.09, 0.13, 0.2, 0.43, 0.5, 0.6, 0.87, 0.91, 1.] {
            let sample = EnergyProfileSample {
                x,
                energy_kcal: 9000.,
            };
            let overlay = g.energy_profile_overlay(
                &style::DEFAULT,
                &source,
                step.from,
                Some((index, sample)),
                None,
            );
            let marker = playback_marker(&overlay);
            let distance = paths
                .iter()
                .flat_map(|path| path.windows(2))
                .map(|pair| {
                    let [a, b] = pair else { unreachable!() };
                    let (dx, dy) = (b.x - a.x, b.y - a.y);
                    let t = (((marker.x - a.x) * dx + (marker.y - a.y) * dy) / (dx * dx + dy * dy))
                        .clamp(0., 1.);
                    (marker.x - a.x - dx * t).hypot(marker.y - a.y - dy * t)
                })
                .fold(f32::INFINITY, f32::min);
            assert!(distance < 0.1, "step {index}, x {x}, distance {distance}");
        }
    }
    for (state, bar) in bars.iter().enumerate() {
        let overlay = g.energy_profile_overlay(&style::DEFAULT, &source, state, None, None);
        let Primitive::Path {
            commands, style, ..
        } = &overlay[0]
        else {
            panic!("Missing playback highlight")
        };
        assert_eq!(*commands, bar.commands);
        assert_eq!(style.width_pt, bar.style.width_pt);
        let [PathCommand::Move(a), PathCommand::Line(b)] = bar.commands.as_slice() else {
            panic!("Expected one level bar segment")
        };
        let expected = a.lerp(*b, 0.5);
        let marker = playback_marker(&overlay);
        assert!((marker.x - expected.x).hypot(marker.y - expected.y) < 0.001);
    }
}

#[test]
fn playback_replaces_only_the_requested_picture_without_changing_the_profile() {
    let g = placed(profile(&[0., 18., -6.]));
    let before = serde_json::to_string(&g).unwrap();
    let source = Document::default();
    let (_, _, pictures) = g.energy_profile_scene(&style::DEFAULT, &source);
    let (_, _, without) = g.energy_profile_scene_without_picture(&style::DEFAULT, &source, 1);
    let p = g.energy_profile.as_deref().unwrap();
    let layout = Layout::with_frame(p, &style::DEFAULT, Some(&g), Some(&source));
    let [reactant, ts, _] = layout.pictures.as_slice() else {
        panic!("Expected three model slots")
    };
    assert_eq!(pictures.len(), without.len() + ts.artwork.len());
    let start = reactant.artwork.len();
    let original = &pictures[start..start + ts.artwork.len()];
    // On the reactant half of the path the moving geometry still replaces the
    // TS picture, leaving the endpoint models at their authored level slots.
    let overlay = |geometry: &EnergyProfileGeometry| {
        g.energy_profile_overlay(
            &style::DEFAULT,
            &source,
            0,
            Some((
                0,
                EnergyProfileSample {
                    x: 0.2,
                    energy_kcal: 4.,
                },
            )),
            Some(geometry),
        )
    };
    let geometry = p.states[1].geometry.as_ref().unwrap();
    let original_overlay = overlay(geometry);
    assert_eq!(original_overlay.len(), 2 + original.len());
    let (expected_lo, expected_hi) = crate::scene::bounds(original);
    assert_eq!(
        crate::scene::bounds(&original_overlay[2..]),
        (expected_lo, expected_hi)
    );

    let mut taller = geometry.clone();
    taller.frame.positions_angstrom[1] = [0., 1.4, 0.2];
    let taller_overlay = overlay(&taller);
    assert!(taller_overlay.len() > 2);
    let (lo, hi) = crate::scene::bounds(&taller_overlay[2..]);
    assert!(hi.x - lo.x <= expected_hi.x - expected_lo.x + 0.001);
    assert!(hi.y - lo.y <= expected_hi.y - expected_lo.y + 0.001);
    assert!(
        lo.lerp(hi, 0.5)
            .distance(expected_lo.lerp(expected_hi, 0.5))
            < 0.001
    );
    assert_eq!(serde_json::to_string(&g).unwrap(), before);
}
