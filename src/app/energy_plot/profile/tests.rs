use super::*;

fn bounds(width: f32, height: f32) -> Rectangle {
    Rectangle::new(Point::ORIGIN, Size::new(width, height))
}

fn fixture(energies: &[f64], saddle: f64) -> Profile {
    let last = energies.len() - 1;
    let highest = energies
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.total_cmp(b))
        .unwrap()
        .0;
    Profile {
        samples: energies
            .iter()
            .enumerate()
            .map(|(index, &energy)| Sample {
                x: index as f64,
                energy,
            })
            .collect(),
        overlay: vec![],
        markers: vec![
            Marker {
                sample: Sample {
                    x: 0.,
                    energy: energies[0],
                },
                label: "R".into(),
                side: Side::Below,
                transition: false,
                note: None,
                connector_from: None,
                tooltip: None,
                thumbnail: Some(0),
            },
            Marker {
                sample: Sample {
                    x: highest as f64,
                    energy: saddle,
                },
                label: "TS‡".into(),
                side: Side::Above,
                transition: true,
                note: Some("Forming C1···C6 1.85 Å · 393i cm⁻¹".into()),
                connector_from: None,
                tooltip: None,
                thumbnail: Some(1),
            },
            Marker {
                sample: Sample {
                    x: last as f64,
                    energy: energies[last],
                },
                label: "P".into(),
                side: Side::Below,
                transition: false,
                note: None,
                connector_from: None,
                tooltip: None,
                thumbnail: Some(2),
            },
        ],
        selected: Some(highest),
        style: Style::Line,
        unit: EnergyUnit::KcalPerMol,
        x_title: "Reaction coordinate".into(),
        x_ticks: false,
        highlight_curve: false,
        stale: false,
    }
}

fn overlaps(a: Rectangle, b: Rectangle) -> bool {
    a.x < b.x + b.width - 0.01
        && a.x + a.width > b.x + 0.01
        && a.y < b.y + b.height - 0.01
        && a.y + a.height > b.y + 0.01
}

#[test]
fn labelled_paths_keep_text_outside_the_curve_at_narrow_and_full_widths() {
    // Rounded NEB energies from the Claisen and Diels–Alder fixtures, plus a flat band.
    for (energies, saddle) in [
        (
            vec![
                0., 0.3, 1.16, 2.30, 3.13, 3.25, 3.06, 7.71, 30.21, 28.52, -0.65, -5.01, -6.76,
                -7.69, -8.17,
            ],
            26.49,
        ),
        (
            vec![
                0., 0.12, 0.44, 0.92, 1.61, 2.66, 4.49, 7.81, 13.83, -30.58, -61.84, -63.38,
                -65.11, -66.96, -67.82,
            ],
            6.97,
        ),
        (vec![0., 0.002, 0.003, 0.001, 0.], 0.0025),
    ] {
        for unit in EnergyUnit::ALL {
            for (width, height) in [
                (330., 146.),
                (330., 190.),
                (330., 210.),
                (420., 210.),
                (560., 260.),
            ] {
                let mut profile = fixture(&energies, saddle);
                profile.unit = unit;
                let bounds = bounds(width, height);
                let layout = profile.layout(bounds);
                assert!(layout.plot.height >= 30., "{width}×{height}: {layout:?}");
                for (index, label) in layout.labels.iter().enumerate() {
                    assert!(
                        !overlaps(label.bounds, layout.plot),
                        "Label over curve: {label:?}"
                    );
                    assert!(label.bounds.x >= 0. && label.bounds.y >= 0.);
                    assert!(label.bounds.x + label.bounds.width <= width + 0.01);
                    assert!(label.bounds.y + label.bounds.height <= height + 0.01);
                    for other in layout.labels.iter().skip(index + 1) {
                        assert!(
                            !overlaps(label.bounds, other.bounds),
                            "{label:?} overlaps {other:?}"
                        );
                    }
                }
                for sample in profile
                    .samples
                    .iter()
                    .chain(profile.markers.iter().map(|marker| &marker.sample))
                {
                    let point = layout.point(*sample);
                    assert!(
                        point.x >= layout.plot.x - 0.01
                            && point.x <= layout.plot.x + layout.plot.width + 0.01
                    );
                    assert!(
                        point.y >= layout.plot.y - 0.01
                            && point.y <= layout.plot.y + layout.plot.height + 0.01
                    );
                }
            }
        }
    }
}

#[test]
fn thumbnails_fit_startup_dock_and_inspector_and_hide_only_in_small_figures() {
    let profile = fixture(&[0., 12., 20., 5., -8.], 19.5);
    for size in [(259., 260.), (560., 139.), (259., 139.)] {
        assert!(profile.layout(bounds(size.0, size.1)).models.is_empty());
    }
    // A 1280 × 820 window gives its bottom-dock chart 180 logical pixels;
    // the inspector is fixed to 320 pixels wide.
    for size in [
        (980., 180.),
        (320., 180.),
        (320., 260.),
        (260., 140.),
        (560., 260.),
    ] {
        let layout = profile.layout(bounds(size.0, size.1));
        assert_eq!(
            layout
                .models
                .iter()
                .map(|model| model.thumbnail)
                .collect::<Vec<_>>(),
            [0, 1, 2]
        );
        for pair in layout.models.windows(2) {
            assert!(!overlaps(pair[0].bounds, pair[1].bounds));
        }
        for model in &layout.models {
            assert!(model.bounds.width >= 36. && model.bounds.height >= 36.);
            assert!(!overlaps(model.bounds, layout.plot));
            for label in &layout.labels {
                assert!(!overlaps(model.bounds, label.bounds));
            }
        }
        assert!(layout.plot.height >= 30., "{size:?}: {layout:?}");
    }
    let mut no_saddle = profile;
    no_saddle.markers.remove(1);
    let layout = no_saddle.layout(bounds(560., 260.));
    assert_eq!(
        layout
            .models
            .iter()
            .map(|model| model.thumbnail)
            .collect::<Vec<_>>(),
        [0, 2]
    );
}

#[test]
fn irc_uses_signed_coordinates_and_overlay_extents_without_changing_trace_values() {
    let mut profile = fixture(&[1.5, 12., 26.5, 4., -7.2], 26.5);
    for (sample, x) in profile.samples.iter_mut().zip([-8., -0.5, 0., 1.2, 9.]) {
        sample.x = x;
    }
    for (marker, x) in profile.markers.iter_mut().zip([-8., 0., 9.]) {
        marker.sample.x = x;
    }
    // Optimized end labels need not be the last IRC frame's numerical energy.
    profile.markers[0].sample.energy = 0.;
    profile.markers[2].sample.energy = -8.44;
    profile.overlay = vec![
        Sample { x: -8., energy: 0. },
        Sample {
            x: 0.,
            energy: 30.2,
        },
        Sample {
            x: 9.,
            energy: -8.44,
        },
    ];
    profile.x_ticks = true;
    profile.highlight_curve = true;
    let layout = profile.layout(bounds(560., 260.));
    assert_eq!(profile.energy_bounds(), (-8.44, 30.2));
    assert_eq!(profile.samples[0].energy, 1.5);
    assert_eq!(profile.samples[4].energy, -7.2);
    assert!(layout.coordinate_ticks.iter().any(|tick| *tick < 0.));
    assert!(layout.coordinate_ticks.contains(&0.));
    assert!(layout.coordinate_ticks.iter().any(|tick| *tick > 0.));
    assert!(layout.point(profile.markers[0].sample).x < layout.point(profile.markers[1].sample).x);
    assert!(layout.point(profile.markers[1].sample).x < layout.point(profile.markers[2].sample).x);
    assert_eq!(coordinate_number(-4.), "−4");
    assert_eq!(coordinate_number(-0.), "0");
}

#[test]
fn compact_irc_keeps_full_notes_in_tooltips_and_ticks_inside_the_allocated_height() {
    let mut profile = fixture(&[0., 16., 23., 9., -8.], 22.5);
    profile.markers[1].note = Some("Forming C1···C6 2.10 Å · C2···C5 2.15 Å · 393i cm⁻¹".into());
    for (sample, x) in profile.samples.iter_mut().zip([-8., -2., 0., 2., 9.]) {
        sample.x = x;
    }
    for (marker, x) in profile.markers.iter_mut().zip([-8., 0., 9.]) {
        marker.sample.x = x;
    }
    profile.overlay = vec![Sample { x: 0., energy: 24. }];
    profile.x_ticks = true;
    profile.highlight_curve = true;
    let layout = profile.layout(bounds(330., 146.));
    assert_eq!(layout.models.len(), 3);
    assert!(layout.plot.height >= 1.);
    assert!(layout.plot.y + layout.plot.height < 146.);
    assert!(layout.footer_legend);
    assert!(layout.legend_y.unwrap() > layout.plot.y + layout.plot.height);
    assert!(layout.coordinate_ticks.contains(&0.));
    for label in &layout.labels {
        assert!(!overlaps(label.bounds, layout.plot));
        assert!(label.bounds.y >= 0. && label.bounds.y + label.bounds.height <= 146.);
    }
    let notes = layout
        .labels
        .iter()
        .filter(|label| label.marker.is_none())
        .map(|label| label.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(notes.is_empty() || notes.ends_with('…'));
    let tooltip = profile
        .hover_text(layout.point(profile.markers[1].sample), &layout)
        .unwrap();
    assert!(tooltip.contains(profile.markers[1].note.as_deref().unwrap()));
}

#[test]
fn coincident_neb_dots_are_painted_after_endpoint_markers() {
    let mut profile = fixture(&[0., 20., -8.], 19.5);
    profile.overlay = vec![profile.markers[0].sample, profile.markers[2].sample];
    let dots = profile.dots_in_draw_order().collect::<Vec<_>>();
    assert!(matches!(dots[0], PlotDot::Marker(_)));
    assert!(matches!(dots[1], PlotDot::Marker(_)));
    assert!(matches!(dots[2], PlotDot::Marker(_)));
    for (dot, endpoint) in dots[3..].iter().zip([0, 2]) {
        let PlotDot::Overlay(sample) = dot else {
            panic!("NEB must paint above marker disks");
        };
        assert_eq!(sample.x, profile.markers[endpoint].sample.x);
        assert_eq!(sample.energy, profile.markers[endpoint].sample.energy);
    }
}

#[test]
fn relaxed_endpoints_have_dotted_vertical_connectors_and_distinct_tooltips() {
    let mut profile = fixture(&[1.5, 20., -7.], 19.5);
    profile.markers[0].connector_from = Some(profile.samples[0]);
    profile.markers[0].sample.energy = 0.;
    profile.markers[0].tooltip = Some("R · relaxed IRC end".into());
    profile.markers[2].connector_from = Some(profile.samples[2]);
    profile.markers[2].sample.energy = -12.;
    profile.overlay = vec![profile.markers[0].sample, profile.markers[2].sample];
    profile.x_ticks = true;
    for size in [(980., 180.), (320., 180.), (320., 260.), (260., 140.)] {
        let layout = profile.layout(bounds(size.0, size.1));
        assert_eq!(layout.models.len(), 3);
        assert_eq!(layout.connectors.len(), 2);
        for connector in &layout.connectors {
            assert_eq!(connector.from.x, connector.to.x);
            assert_ne!(connector.from.y, connector.to.y);
            assert!(connector.from.y >= layout.plot.y);
            assert!(connector.to.y <= layout.plot.y + layout.plot.height);
            let midpoint = connector.from + (connector.to - connector.from) * 0.5;
            assert!(
                profile
                    .hover_text(midpoint, &layout)
                    .unwrap()
                    .contains(RELAXED_LEGEND)
            );
        }
        let tooltip = profile
            .hover_text(layout.point(profile.markers[0].sample), &layout)
            .unwrap();
        assert!(tooltip.contains("relaxed IRC end"));
        assert!(tooltip.contains(RELAXED_LEGEND));
        assert!(layout.legend_y.is_some());
        for label in &layout.labels {
            assert!(!overlaps(label.bounds, layout.plot));
            assert!(label.bounds.y + label.bounds.height <= size.1);
        }
    }
    let stroke = relaxation_stroke(Color::BLACK);
    assert_eq!(stroke.line_dash.segments, &[1., 3.]);
    assert_eq!(RELAXED_LEGEND, "relaxed to the minimum");
    assert_eq!(profile.samples[0].energy, 1.5);
    assert_eq!(profile.samples[2].energy, -7.);
}

#[test]
fn selection_uses_irregular_spacing_and_skips_nonfinite_samples() {
    let mut profile = fixture(&[0., 1., 2., 3., 4.], 3.);
    profile.markers.clear();
    for (sample, x) in profile.samples.iter_mut().zip([-8., -0.5, 0., 1.2, 9.]) {
        sample.x = x;
    }
    let bounds = bounds(420., 210.);
    assert_eq!(profile.nearest(profile.x(-0.6, bounds), bounds), Some(1));
    assert_eq!(profile.nearest(profile.x(0.8, bounds), bounds), Some(3));
    assert_eq!(profile.nearest(0., bounds), Some(0));
    assert_eq!(profile.nearest(bounds.width, bounds), Some(4));
    profile.samples[3].energy = f64::NAN;
    assert_eq!(profile.nearest(profile.x(0.8, bounds), bounds), Some(2));
    profile.samples.clear();
    assert_eq!(profile.nearest(100., bounds), None);
}

#[test]
fn levels_share_the_labelled_style_and_leave_room_for_terminal_level_widths() {
    let mut profile = fixture(&[0., 20., -8.], 19.5);
    profile.style = Style::Levels;
    profile.markers[0].label = "Starting material".into();
    profile.markers[2].label = "Intermediate 1".into();
    let layout = profile.layout(bounds(420., 260.));
    let first = layout.point(profile.samples[0]);
    let last = layout.point(profile.samples[2]);
    assert!(first.x - 22. >= layout.plot.x);
    assert!(last.x + 22. <= layout.plot.x + layout.plot.width);
    assert!(
        layout
            .labels
            .iter()
            .any(|label| label.text.starts_with("Intermediate 1"))
    );
}

#[test]
fn grid_steps_are_round_in_each_displayed_energy_unit() {
    for (unit, expected_step) in [
        (EnergyUnit::KcalPerMol, 10.),
        (EnergyUnit::KjPerMol, 50.),
        (EnergyUnit::Ev, 0.5),
    ] {
        let mut profile = fixture(&[0., 27.3, -8.44], 26.5);
        profile.unit = unit;
        let layout = profile.layout(bounds(560., 260.));
        let step = unit.convert_kcal(layout.energy_ticks[1] - layout.energy_ticks[0]);
        assert!((step - expected_step).abs() < 1e-9, "{unit:?}: {step}");
        assert!(layout.energy_ticks.iter().any(|tick| tick.abs() < 1e-9));
    }
}
