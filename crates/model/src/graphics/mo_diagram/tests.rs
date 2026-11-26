use super::*;
use crate::document::CALCULATIONS;
use crate::models3d::surface::{OrbitalRef, Snapshot};

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

fn picture(result: &str, spin: Spin, index: u32) -> Figure {
    Figure {
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
    let g = placed(mo, &style::DEFAULT);
    let (parts, labels) = g.mo_parts(&style::DEFAULT);
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
    assert_eq!(
        mo.level_at_with_style(&g, point, &style),
        Some((Spin::Alpha, 1))
    );
    let label = layout
        .labels
        .iter()
        .find(|l| l.orbital == Some((Spin::Alpha, 2)))
        .unwrap();
    assert_eq!(
        mo.level_at_with_style(&g, layout.annotation(&g, label).centre(), &style),
        Some((Spin::Alpha, 2))
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
        picture("old", Spin::Alpha, 1),
        picture("old", Spin::Alpha, 3),
        picture("old", Spin::Alpha, 5),
    ];
    mo.pictures[0].look.opacity = 70;
    let appearance = mo.pictures[0].look;
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
            .map(|p| p.orbital.index)
            .collect::<Vec<_>>(),
        vec![2, 4]
    );
    assert_eq!(mo.pictures[0].look, appearance);
    assert_eq!(mo.pictures[0].iso, 0.07);
    assert!(
        mo.pictures
            .iter()
            .all(|p| p.snapshot.is_none() && p.orbital.result == "new")
    );
}

#[test]
fn relink_follows_the_beta_column_when_its_rows_become_shared() {
    let mut mo = MoDiagram::new(
        1,
        "old".into(),
        vec![
            column(Spin::Alpha, "α", &[-12., -10., -8.], 2),
            column(Spin::Beta, "β", &[-13., -9., -4.], 1),
        ],
    );
    mo.pictures = vec![picture("old", Spin::Beta, 1)];
    mo.relink(
        "shared".into(),
        vec![
            column(Spin::Alpha, "α", &[-14., -12., -10., -8.], 3),
            column(Spin::Alpha, "β", &[-14., -12., -10., -8.], 2),
        ],
    );
    assert_eq!(mo.pictures[0].orbital.spin, Spin::Alpha);
    assert_eq!(mo.pictures[0].orbital.index, 2);
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
    let variants: [fn(&mut MoDiagram); 8] = [
        |mo| mo.columns.clear(),
        |mo| mo.columns[0].energies_ev[0] = f64::INFINITY,
        |mo| mo.columns[0].energies_ev.resize(4097, 0.),
        |mo| mo.columns[0].occupations.push(1.),
        |mo| mo.below = 9,
        |mo| mo.result = "x".repeat(65),
        |mo| mo.pictures = vec![picture("old", Spin::Alpha, 0); 17],
        |mo| mo.pictures = vec![picture("old", Spin::Beta, 2)],
    ];
    for perturb in variants {
        let mut invalid = g.clone();
        perturb(invalid.mo.as_deref_mut().unwrap());
        assert!(invalid.validate().is_err());
    }
    doc.graphics[0].mo = None;
    assert!(doc.validate().is_err());
}
