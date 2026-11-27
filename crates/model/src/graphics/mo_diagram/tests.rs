use super::*;
use crate::document::CALCULATIONS;
use crate::models3d::surface::{OrbitalRef, Snapshot};
use reshiki_qcdata::Spin;

fn column(spin: Spin, label: &str, energies: &[f64], occupied: usize) -> Column {
    Column {
        spin,
        label: label.into(),
        energies_ev: energies.to_vec(),
        occupations: vec![],
        occupied: Some(occupied),
    }
}

fn benzene() -> MoDiagram {
    MoDiagram::new(
        1,
        "old".into(),
        vec![column(
            Spin::Alpha,
            "",
            &[-13.8, -10.95, -10.95, -6.02, -6.02, -2.1],
            3,
        )],
    )
}

fn placed(mo: MoDiagram, style: &DrawingStyle) -> Graphic {
    let (w, h) = mo.natural_size(style);
    Graphic::mo_diagram(
        2,
        Point::new(4., 8.),
        Point::new(w, h),
        mo,
        GraphicStyle {
            width_pt: style.line_width_pt,
            ..GraphicStyle::default()
        },
    )
}

fn read_native(graphic: Graphic) -> Result<Document, String> {
    let doc = Document {
        version: CALCULATIONS,
        graphics: vec![graphic],
        ..Document::default()
    };
    Document::from_json(&serde_json::to_vec(&doc).unwrap())
}

fn picture(result: &str, column: u8, spin: Spin, index: u32) -> MoPicture {
    MoPicture {
        column,
        figure: Figure {
            orbital: OrbitalRef {
                result: result.into(),
                spin,
                index,
            },
            iso: 0.07,
            look: Default::default(),
            snapshot: Some(Snapshot {
                view: "before".into(),
                reach: 1.,
                shapes: "old".into(),
            }),
        },
    }
}

#[test]
fn closed_shell_labels_and_orders_degenerate_bars() {
    let mo = benzene();
    let layout = Layout::new(&mo, &style::DEFAULT);
    let c = &layout.columns[0];
    assert_eq!(
        c.groups
            .iter()
            .map(|g| g.levels.clone())
            .collect::<Vec<_>>(),
        vec![5..=5, 3..=4, 1..=2, 0..=0]
    );
    assert!(
        c.groups
            .windows(2)
            .all(|pair| pair[0].energy > pair[1].energy && pair[0].y < pair[1].y)
    );
    let em = layout.font.size();
    let bars: Vec<_> = c.bars(&c.groups[2], em).collect();
    assert_eq!(bars.len(), 2);
    assert!(bars[0].2 < bars[1].1);
    assert!((bars[0].2 - bars[0].1 - 2.5 * em).abs() < 0.001);
    let mut g = placed(mo.clone(), &style::DEFAULT);
    g.style.width_pt = 12.;
    let (parts, labels) = g.mo_parts(&style::DEFAULT);
    for (_, left, right) in bars {
        let y = c.groups[2].y;
        let commands = [
            PathCommand::Move(layout.world(&g, Point::new(left, y))),
            PathCommand::Line(layout.world(&g, Point::new(right, y))),
        ];
        let bar = parts.iter().find(|part| part.commands == commands).unwrap();
        assert_eq!(bar.style.width_pt, 12.);
        assert!(!bar.style.round_caps());
    }
    let texts: Vec<_> = labels.iter().map(|a| a.text.as_str()).collect();
    assert!(texts.contains(&"HOMO  −10.95 eV · 2-fold"));
    assert!(texts.contains(&"LUMO  −6.02 eV · 2-fold"));
    assert!(texts.contains(&"gap 4.93 eV"));
    assert_eq!(&texts[..5], &["eV", "−14", "−10", "−6", "−2"]);
    // Occupations fall back to the occupied-level count. Every filled bar has two arrows.
    assert_eq!(
        parts.iter().filter(|part| part.commands.len() == 3).count(),
        6
    );
}

#[test]
fn range_includes_whole_groups_and_separates_close_levels() {
    let mut mo = benzene();
    mo.below = 0;
    mo.above = 0;
    let layout = Layout::new(&mo, &style::DEFAULT);
    assert_eq!(layout.columns[0].groups[0].levels, 3..=4);
    assert_eq!(layout.columns[0].groups[1].levels, 1..=2);
    mo.columns[0].energies_ev = (0..18).map(|i| -1. + f64::from(i) * 0.006).collect();
    mo.columns[0].occupied = Some(9);
    mo.below = 8;
    mo.above = 8;
    let layout = Layout::new(&mo, &style::DEFAULT);
    let groups = &layout.columns[0].groups;
    assert_eq!(groups.len(), 18);
    let em = layout.font.size();
    assert!(groups.iter().all(|g| g.y > em && g.y < layout.height - em));
    assert!(
        groups
            .windows(2)
            .all(|pair| pair[1].y - pair[0].y >= 1.45 * em - 0.001)
    );
    mo.columns[0].occupied = None;
    let layout = Layout::new(&mo, &style::DEFAULT);
    assert_eq!(layout.columns[0].groups.len(), 5);
}

#[test]
fn open_shell_draws_two_columns_with_only_frontier_labels() {
    let mo = MoDiagram::new(
        1,
        "old".into(),
        vec![
            column(Spin::Alpha, "α", &[-12., -10., -8., -3.], 2),
            column(Spin::Beta, "β", &[-13., -9., -4., -2.], 1),
        ],
    );
    let layout = Layout::new(&mo, &style::DEFAULT);
    assert!(layout.columns[0].left < layout.columns[1].left);
    let g = placed(mo, &style::DEFAULT);
    let (parts, labels) = g.mo_parts(&style::DEFAULT);
    let texts: Vec<_> = labels.iter().map(|a| a.text.as_str()).collect();
    assert!(texts.contains(&"α") && texts.contains(&"β"));
    assert!(texts.contains(&"gap 1.00 eV"));
    assert_eq!(
        texts
            .iter()
            .filter(|s| s.starts_with("α ") || s.starts_with("β "))
            .copied()
            .collect::<Vec<_>>(),
        vec![
            "α LUMO  −8.00 eV",
            "α HOMO  −10.00 eV",
            "β LUMO  −9.00 eV",
            "β HOMO  −13.00 eV"
        ]
    );
    let arrows: Vec<_> = parts
        .iter()
        .filter_map(|part| match part.commands.as_slice() {
            [
                PathCommand::Move(tail),
                PathCommand::Line(tip),
                PathCommand::Line(_),
            ] => Some(tip.y > tail.y),
            _ => None,
        })
        .collect();
    assert_eq!(arrows, vec![false, false, true]);
}

#[test]
fn open_shell_rows_keep_the_gap_and_both_columns_in_energy_order() {
    let mo = MoDiagram::new(
        1,
        "old".into(),
        vec![
            column(Spin::Alpha, "α", &[-7., -6., -5., -1., 2., 5.], 3),
            column(Spin::Beta, "β", &[-8., -4.5, 6., 7., 8., 9.], 1),
        ],
    );
    let layout = Layout::new(&mo, &style::DEFAULT);
    let mut groups: Vec<_> = layout.columns.iter().flat_map(|c| &c.groups).collect();
    groups.sort_by(|a, b| b.energy.total_cmp(&a.energy));
    assert!(groups.windows(2).all(|pair| pair[0].y < pair[1].y));
    let spacing = 1.45 * layout.font.size();
    for c in &layout.columns {
        assert!(
            c.groups
                .windows(2)
                .all(|pair| pair[1].y - pair[0].y >= spacing - 0.001)
        );
    }
    let homo = group_y(&layout.columns, (0, 2)).unwrap();
    let lumo = group_y(&layout.columns, (1, 1)).unwrap();
    assert!(lumo < homo);
    assert!(layout.lines.iter().any(|line| {
        matches!(line.as_slice(), [_, start, end, _] if start.y == homo && end.y == lumo)
    }));
    assert!(
        layout
            .labels
            .iter()
            .any(|label| label.text == "gap 0.50 eV")
    );
}

#[test]
fn nearby_spin_groups_share_a_row_without_merging_alpha_groups() {
    let mo = MoDiagram::new(
        1,
        "old".into(),
        vec![
            column(Spin::Alpha, "α", &[-9., -5.008, -5., -1.], 2),
            column(Spin::Beta, "β", &[-8., -5.004, 0.], 1),
        ],
    );
    let layout = Layout::new(&mo, &style::DEFAULT);
    let beta_row = group_y(&layout.columns, (1, 1)).unwrap();
    assert_eq!(group_y(&layout.columns, (0, 2)), Some(beta_row));
    let alpha = &layout.columns[0];
    let group = alpha.groups.iter().find(|g| g.y == beta_row).unwrap();
    assert_eq!(group.levels, 2..=2);
    let bars: Vec<_> = alpha.bars(group, layout.font.size()).collect();
    assert!(matches!(bars.as_slice(), [(2, _, _)]));
    let homo = group_y(&layout.columns, (0, 1)).unwrap();
    assert!(beta_row < homo);
    assert!(layout.lines.iter().any(|line| {
        matches!(line.as_slice(), [_, start, end, _] if start.y == homo && end.y == beta_row)
    }));
}

#[test]
fn beta_without_occupations_keeps_the_alpha_frontier_rows_apart() {
    let mo = MoDiagram::new(
        1,
        "old".into(),
        vec![
            column(Spin::Alpha, "α", &[-5.008, -5.000], 1),
            Column {
                occupied: None,
                ..column(Spin::Beta, "β", &[-5.004], 0)
            },
        ],
    );
    let layout = Layout::new(&mo, &style::DEFAULT);
    let homo = group_y(&layout.columns, (0, 0)).unwrap();
    let lumo = group_y(&layout.columns, (0, 1)).unwrap();
    let beta = group_y(&layout.columns, (1, 0)).unwrap();
    assert!(lumo < homo);
    assert!(beta == lumo || beta == homo);
    // Separate spins without β occupations have no gap, as in the Summary.
    assert!(
        !layout
            .labels
            .iter()
            .any(|label| label.text.starts_with("gap"))
    );
    assert!(
        !layout
            .labels
            .iter()
            .any(|label| label.text.contains("fold"))
    );
}

#[test]
fn text_uses_the_smaller_frame_scale() {
    let mut g = placed(benzene(), &style::DEFAULT);
    let (_, natural) = g.mo_parts(&style::DEFAULT);
    for (x, y) in [(0.5, 1.), (1., 0.5)] {
        let (axis_x, axis_y) = (g.axis_x, g.axis_y);
        g.axis_x.x *= x;
        g.axis_y.y *= y;
        let (_, compressed) = g.mo_parts(&style::DEFAULT);
        assert_eq!(compressed.len(), natural.len());
        for (a, b) in natural.iter().zip(compressed) {
            assert_eq!(a.text, b.text);
            assert!((a.format.style.size_pt * 0.5 - b.format.style.size_pt).abs() < 0.001);
        }
        (g.axis_x, g.axis_y) = (axis_x, axis_y);
    }
}

#[test]
fn level_at_identifies_a_rotated_bar_and_label() {
    let style = DrawingStyle {
        font_family: "Georgia".into(),
        font_size_pt: 14.,
        ..style::DEFAULT.clone()
    };
    let mut g = placed(benzene(), &style);
    let origin = g.origin;
    g.map_positions(|p| p.turned(origin, 35.));
    assert!(g.edit_points().is_empty());
    let mo = g.mo.as_deref().unwrap();
    let layout = Layout::new(mo, &style);
    let c = &layout.columns[0];
    let group = &c.groups[2];
    let (_, left, right) = c.bars(group, layout.font.size()).next().unwrap();
    let point = layout.world(&g, Point::new((left + right) / 2., group.y));
    assert_eq!(mo.level_at_with_style(&g, point, &style), Some((0, 1)));
    let label = layout
        .labels
        .iter()
        .find(|l| l.orbital == Some((0, 2)))
        .unwrap();
    assert_eq!(
        mo.level_at_with_style(&g, layout.annotation(&g, label).centre(), &style),
        Some((0, 2))
    );
    let blank = layout.world(&g, Point::new(1., 1.));
    assert!(g.hit(blank, 0.));
    assert_eq!(mo.level_at_with_style(&g, blank, &style), None);
    assert_eq!(mo.level_at(&g, Point::new(-1000., -1000.)), None);
    for corner in g.commands().iter().flat_map(PathCommand::iter_points) {
        let (low, high) = g.shape_bounds();
        assert!((low.x..=high.x).contains(&corner.x) && (low.y..=high.y).contains(&corner.y));
    }
}

#[test]
fn relink_keeps_frontier_choices_range_and_look() {
    let mut mo = benzene();
    mo.below = 4;
    mo.above = 6;
    mo.linked = false;
    mo.pictures = vec![
        picture("old", 0, Spin::Alpha, 1),
        picture("old", 0, Spin::Alpha, 3),
        picture("old", 0, Spin::Alpha, 5),
    ];
    mo.pictures[0].figure.look.opacity = 70;
    let appearance = mo.pictures[0].figure.look;
    let next = vec![column(
        Spin::Alpha,
        "",
        &[-15., -13., -11., -9., -6., -4.],
        4,
    )];
    mo.relink("new".into(), next.clone());
    assert_eq!(mo.result, "new");
    assert_eq!(mo.columns, next);
    assert_eq!((mo.below, mo.above, mo.linked), (4, 6, false));
    assert_eq!(
        mo.pictures
            .iter()
            .map(|p| p.figure.orbital.index)
            .collect::<Vec<_>>(),
        vec![2, 4]
    );
    assert_eq!(mo.pictures[0].figure.look, appearance);
    assert_eq!(mo.pictures[0].figure.iso, 0.07);
    assert!(
        mo.pictures
            .iter()
            .all(|p| p.figure.snapshot.is_none() && p.figure.orbital.result == "new")
    );
}

#[test]
fn shared_rows_hit_and_relink_by_column() {
    let mut mo = MoDiagram::new(
        1,
        "old".into(),
        vec![
            column(Spin::Alpha, "α", &[-12., -10., -8.], 2),
            column(Spin::Alpha, "β", &[-12., -10., -8.], 1),
        ],
    );
    mo.pictures = vec![
        picture("old", 0, Spin::Alpha, 1),
        picture("old", 1, Spin::Alpha, 1),
    ];
    let g = placed(mo.clone(), &style::DEFAULT);
    let layout = Layout::new(&mo, &style::DEFAULT);
    for c in &layout.columns {
        let group = c.groups.iter().find(|g| g.levels.contains(&1)).unwrap();
        let (_, left, right) = c.bars(group, layout.font.size()).next().unwrap();
        let point = layout.world(&g, Point::new((left + right) / 2., group.y));
        assert_eq!(mo.level_at(&g, point), Some((c.index, 1)));
        let slot = &layout.pictures[usize::from(c.index)];
        assert!(slot.connector.x > c.left && slot.centre.x > slot.connector.x);
        assert_eq!(slot.centre.y, group.y);
    }
    assert!(layout.pictures[0].centre.x < layout.columns[1].left);
    mo.relink(
        "new".into(),
        vec![
            column(Spin::Beta, "β", &[-14., -12., -10., -8.], 2),
            column(Spin::Alpha, "α", &[-14., -12., -10., -8.], 2),
        ],
    );
    assert_eq!(mo.pictures[0].column, 1);
    assert_eq!(mo.pictures[0].figure.orbital.index, 1);
    assert_eq!(mo.pictures[1].column, 0);
    assert_eq!(mo.pictures[1].figure.orbital.spin, Spin::Beta);
    assert_eq!(mo.pictures[1].figure.orbital.index, 2);
}

#[test]
fn native_version_and_validation_cover_mo_diagram_data() {
    let g = placed(benzene(), &style::DEFAULT);
    let mut doc = Document {
        graphics: vec![g.clone()],
        ..Document::default()
    };
    assert_eq!(doc.file_version(), CALCULATIONS);
    assert!(doc.validate().is_ok());
    let variants: [fn(&mut MoDiagram); 10] = [
        |mo| mo.columns.clear(),
        |mo| mo.columns[0].energies_ev[0] = f64::INFINITY,
        |mo| mo.columns[0].energies_ev.resize(4097, 0.),
        |mo| mo.columns[0].occupations.push(1.),
        |mo| mo.below = 9,
        |mo| mo.result = "x".repeat(65),
        |mo| mo.pictures = vec![picture("old", 0, Spin::Alpha, 0); 17],
        |mo| mo.pictures = vec![picture("old", 0, Spin::Beta, 2)],
        |mo| mo.pictures = vec![picture("old", 1, Spin::Alpha, 2)],
        |mo| mo.pictures = vec![picture("old", 0, Spin::Alpha, 6)],
    ];
    for perturb in variants {
        let mut invalid = g.clone();
        perturb(invalid.mo.as_deref_mut().unwrap());
        assert!(invalid.validate().is_err());
    }
    doc.graphics[0].mo = None;
    assert!(doc.validate().is_err());
}

#[test]
fn native_read_requires_closed_or_ordered_open_shell_labels() {
    for (labels, valid) in [
        (&[""][..], true),
        (&["α", "β"], true),
        (&["α"], false),
        (&["β"], false),
        (&["restricted"], false),
        (&["β", "α"], false),
        (&["α", "α"], false),
        (&["", "β"], false),
    ] {
        let mut g = placed(benzene(), &style::DEFAULT);
        g.mo.as_mut().unwrap().columns = labels
            .iter()
            .map(|label| column(Spin::Alpha, label, &[-6., -4.], 1))
            .collect();
        assert_eq!(read_native(g).is_ok(), valid, "labels {labels:?}");
    }
}

#[test]
fn a_partial_open_shell_keeps_both_columns_and_relinks_the_available_picture() {
    let alpha = column(Spin::Alpha, "α", &[-7., -5., -3.], 1);
    let beta = column(Spin::Beta, "β", &[-6., -4., -2.], 1);
    let mut mo = MoDiagram::new(1, "old".into(), vec![beta.clone()]);
    assert_eq!(mo.columns[0].label, "α");
    assert!(mo.columns[0].energies_ev.is_empty());
    assert_eq!(mo.columns[1], beta);
    mo.pictures.push(picture("old", 1, Spin::Beta, 0));
    mo.relink("full".into(), vec![alpha.clone(), beta.clone()]);
    assert_eq!(mo.columns[0], alpha);
    mo.relink("partial".into(), vec![beta]);
    assert_eq!(mo.pictures.len(), 1);
    assert_eq!(mo.pictures[0].column, 1);
    assert_eq!(mo.pictures[0].figure.orbital.result, "partial");
    assert!(read_native(placed(mo, &style::DEFAULT)).is_ok());
}

#[test]
fn native_read_preserves_equal_energies_and_rejects_descending_ones() {
    let mut g = placed(benzene(), &style::DEFAULT);
    assert!(read_native(g.clone()).is_ok());
    g.mo.as_mut().unwrap().columns[0].energies_ev = vec![-10., -5., -10.];
    assert_eq!(read_native(g).unwrap_err(), "Invalid MO diagram columns");
}

#[test]
fn native_read_requires_a_finite_nonsingular_frame() {
    let mut g = placed(benzene(), &style::DEFAULT);
    let origin = g.origin;
    g.map_positions(|p| p.turned(origin, 37.));
    assert!(read_native(g.clone()).is_ok());
    g.map_positions(|p| Point::new(-p.x, p.y));
    assert!(g.frame_determinant() < 0.);
    assert!(read_native(g.clone()).is_ok());
    assert!(
        read_native(Graphic {
            axis_x: Point::new(1., 0.),
            axis_y: Point::new(0.8, 0.6),
            ..g.clone()
        })
        .is_ok()
    );

    for (description, origin, axis_x, axis_y) in [
        ("zero axis", origin, Point::default(), Point::new(0., 1.)),
        (
            "tiny axis",
            origin,
            Point::new(1e-4, 0.),
            Point::new(0., 1.),
        ),
        (
            "collinear",
            origin,
            Point::new(2., 2.),
            Point::new(-1., -1.),
        ),
        (
            "nearly parallel",
            origin,
            Point::new(1e4, 0.),
            Point::new(1e4, 1e-3),
        ),
        (
            "overflowing length",
            origin,
            Point::new(1., 0.),
            Point::new(3e38, 3e38),
        ),
        (
            "overflowing determinant",
            origin,
            Point::new(1e20, 0.),
            Point::new(0., 1e20),
        ),
        (
            "overflowing corner",
            Point::new(f32::MAX, 0.),
            Point::new(1e38, 0.),
            Point::new(0., 1.),
        ),
    ] {
        let invalid = Graphic {
            origin,
            axis_x,
            axis_y,
            ..g.clone()
        };
        assert!(
            read_native(invalid)
                .unwrap_err()
                .contains("independent axes"),
            "{description}"
        );
    }
}

#[test]
fn a_finite_wide_frame_keeps_a_finite_picture_scale() {
    let mut g = placed(benzene(), &style::DEFAULT);
    g.axis_x = Point::new(3e38, 0.);
    g.axis_y = Point::new(0., 1.);
    assert!(read_native(g.clone()).is_ok());
    let layout = Layout::new(g.mo.as_deref().unwrap(), &style::DEFAULT);
    let side = layout.picture_world_side(&g);
    assert!(side.is_finite() && side > 0.);
}

#[test]
fn a_group_below_the_homo_is_named_by_its_level_nearest_the_gap() {
    let mo = MoDiagram::new(
        1,
        "old".into(),
        vec![column(
            Spin::Alpha,
            "",
            &[-11.47, -11.47, -10.95, -10.95, -6.02, -6.02, -2.45],
            4,
        )],
    );
    let (_, labels) = placed(mo, &style::DEFAULT).mo_parts(&style::DEFAULT);
    let texts: Vec<_> = labels.iter().map(|a| a.text.as_str()).collect();
    assert!(texts.contains(&"HOMO−2  −11.47 eV · 2-fold"), "{texts:?}");
    assert!(!texts.iter().any(|t| t.starts_with("HOMO−3")));
}

#[test]
fn shared_open_shell_orbitals_take_the_alpha_gap() {
    // Triplet O2 in GFN2-xTB: both spins share the levels; α fills the π* pair.
    let energies = [-20.1, -17.37, -17.37, -12.05, -12.05, 3.36];
    let mo = MoDiagram::new(
        1,
        "old".into(),
        vec![
            column(Spin::Alpha, "α", &energies, 5),
            column(Spin::Alpha, "β", &energies, 3),
        ],
    );
    let (_, labels) = placed(mo, &style::DEFAULT).mo_parts(&style::DEFAULT);
    let texts: Vec<_> = labels.iter().map(|a| a.text.as_str()).collect();
    assert!(texts.contains(&"gap 15.41 eV"), "{texts:?}");
    assert!(!texts.iter().any(|t| t.contains("−0.00")));
}

#[test]
fn the_gap_follows_the_summary_rule_for_separate_spins() {
    let gap = |mo: MoDiagram| -> Option<String> {
        let (_, labels) = placed(mo, &style::DEFAULT).mo_parts(&style::DEFAULT);
        labels
            .into_iter()
            .find(|a| a.text.starts_with("gap"))
            .map(|a| a.text)
    };
    let ev = [-16.33, -10.88, 5.44];
    let unrestricted = |alpha: usize, beta: Option<usize>| {
        let mut b = column(Spin::Beta, "β", &ev, beta.unwrap_or(0));
        b.occupied = beta;
        MoDiagram::new(
            1,
            "old".into(),
            vec![column(Spin::Alpha, "α", &ev, alpha), b],
        )
    };
    // Equal energies but separate spins: the extremes of both.
    assert_eq!(
        gap(unrestricted(2, Some(1))).as_deref(),
        Some("gap 0.00 eV")
    );
    // xtb68 conventions take the α frontier.
    let mut mo = unrestricted(2, Some(1));
    mo.alpha_gap = true;
    assert_eq!(gap(mo).as_deref(), Some("gap 16.32 eV"));
    // A spin without occupations: no gap, as the Summary omits it.
    assert_eq!(gap(unrestricted(2, None)), None);
    // An H atom: α's only level filled, β's empty; neither spin has a whole frontier.
    let h = MoDiagram::new(
        1,
        "old".into(),
        vec![
            column(Spin::Alpha, "α", &[-13.6], 1),
            column(Spin::Beta, "β", &[-10.88], 0),
        ],
    );
    assert_eq!(gap(h), None);
}

#[test]
fn electrons_cross_their_bar_up_left_and_down_right() {
    let (parts, _) = placed(benzene(), &style::DEFAULT).mo_parts(&style::DEFAULT);
    let segment = |part: &Part| match part.commands.as_slice() {
        [PathCommand::Move(a), PathCommand::Line(b)] => Some((*a, *b)),
        _ => None,
    };
    let bars: Vec<_> = parts
        .iter()
        .filter(|p| p.style.flat_caps)
        .filter_map(segment)
        .filter(|(a, b)| (a.y - b.y).abs() < 1e-3)
        .collect();
    let shafts: Vec<_> = parts
        .iter()
        .filter_map(segment)
        .filter(|(a, b)| (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() > 1e-3)
        // The energy axis is vertical too; electrons lie within a bar.
        .filter(|(a, _)| bars.iter().any(|(l, r)| a.x > l.x && a.x < r.x))
        .collect();
    // Three filled bars, two electrons each.
    assert_eq!(shafts.len(), 6);
    for (left, right) in bars {
        let on: Vec<_> = shafts
            .iter()
            .filter(|(tail, tip)| {
                tail.x > left.x
                    && tail.x < right.x
                    && tail.y.min(tip.y) < left.y
                    && tail.y.max(tip.y) > left.y
            })
            .collect();
        if on.is_empty() {
            continue;
        }
        let centre = (left.x + right.x) / 2.;
        let [(up_tail, up_tip), (down_tail, down_tip)] = [on[0], on[1]].map(|s| *s);
        for (tail, tip) in [(up_tail, up_tip), (down_tail, down_tip)] {
            assert!(
                ((tail.y + tip.y) / 2. - left.y).abs() < 1e-3,
                "centred on the bar"
            );
        }
        // Screen y grows downward: ↑ has its tip above its tail.
        assert!(up_tip.y < up_tail.y && up_tail.x < centre);
        assert!(down_tip.y > down_tail.y && down_tail.x > centre);
        assert!(((centre - up_tail.x) - (down_tail.x - centre)).abs() < 1e-3);
    }
}
