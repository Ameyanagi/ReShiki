//! Captured-baseline parity for Arrow::paths and the bounds and hit tests built
//! on it. Run with RESHIKI_MODEL_PARITY set; see crate::parity_tests.
use super::{ArrowStyle, Head, HeadShape, NoGo, Preset};
use crate::{
    document::{Arrow, Point},
    graphics::LinePattern,
    parity_tests::{check_baseline, measured},
};
use std::fmt::Write as _;

#[derive(Debug, Clone, Copy)]
enum Control {
    Default,
    Above,
    Below,
}

fn geometries() -> [(&'static str, Point, Point); 4] {
    [
        ("horizontal", Point::new(0., 0.), Point::new(120., 0.)),
        ("reversed", Point::new(120., 0.), Point::new(0., 0.)),
        ("diagonal", Point::new(10., 20.), Point::new(95., -65.)),
        // Shorter than the arrowhead, so every `distance * 0.4` clamp applies.
        ("short", Point::new(30., 40.), Point::new(36., 40.)),
    ]
}

fn arrow(
    preset: Preset,
    style: Option<ArrowStyle>,
    start: Point,
    end: Point,
    control: Control,
) -> Arrow {
    let mut arrow = Arrow::new(1, start, end, preset, ArrowStyle::preset(preset));
    arrow.style = style;
    let bend = |k: f32| {
        Point::new(
            (start.x + end.x) * 0.5 - (end.y - start.y) * k,
            (start.y + end.y) * 0.5 + (end.x - start.x) * k,
        )
    };
    arrow.control = match control {
        Control::Default => None,
        Control::Above => Some(bend(-0.3)),
        Control::Below => Some(bend(0.3)),
    };
    arrow
}

fn sweeps(base: &ArrowStyle) -> Vec<(String, ArrowStyle)> {
    let with = |label: String, edit: &dyn Fn(&mut ArrowStyle)| {
        let mut style = base.clone();
        edit(&mut style);
        (label, style)
    };
    let mut out = Vec::new();
    for &head in Head::ALL {
        out.push(with(format!("head={head:?}"), &|s| s.head = head));
    }
    for &tail in Head::ALL {
        out.push(with(format!("tail={tail:?}"), &|s| s.tail = tail));
    }
    for &shape in HeadShape::ALL {
        out.push(with(format!("shape={shape:?}"), &|s| s.shape = shape));
    }
    for &no_go in NoGo::ALL {
        out.push(with(format!("no_go={no_go:?}"), &|s| s.no_go = no_go));
    }
    for dipole in [false, true] {
        out.push(with(format!("dipole={dipole}"), &|s| s.dipole = dipole));
    }
    for pattern in [LinePattern::Solid, LinePattern::Dashed, LinePattern::Dotted] {
        out.push(with(format!("pattern={pattern:?}"), &|s| {
            s.pattern = pattern
        }));
    }
    for ratio in [1.0, 0.6] {
        out.push(with(format!("equilibrium_ratio={ratio:?}"), &|s| {
            s.equilibrium_ratio = ratio
        }));
    }
    for notch in [0., 0.125, 0.5] {
        out.push(with(format!("head_notch={notch:?}"), &|s| {
            s.head_notch = notch
        }));
    }
    out
}

fn record(text: &mut String, label: &str, arrow: &Arrow) {
    writeln!(text, "case {label}").unwrap();
    for path in arrow.paths() {
        writeln!(text, "{:?} {:?} {}", path.commands, path.style, path.filled).unwrap();
    }
    writeln!(text, "bounds {:?}", arrow.bounds()).unwrap();
    for probe in [
        arrow.point(0.5),
        arrow.end.offset(1., 1.),
        arrow.start.offset(-3., 0.5),
    ] {
        writeln!(text, "hit {probe:?} {}", arrow.hit(probe, 0.75)).unwrap();
    }
    // The paths above warm the call; measure a second, identical one.
    let (paths, allocations) = measured(|| arrow.paths());
    drop(paths);
    writeln!(text, "alloc {allocations}").unwrap();
}

#[test]
#[ignore = "Captured-baseline parity; set RESHIKI_MODEL_PARITY"]
fn arrow_paths_match_captured_baseline() {
    let mut text = String::new();
    let controls = [Control::Default, Control::Above, Control::Below];
    // Every preset, styled and legacy unstyled, at every geometry and bend.
    for &preset in Preset::ALL {
        for (geometry, start, end) in geometries() {
            for control in controls {
                for style in [Some(ArrowStyle::preset(preset)), None] {
                    let source = if style.is_some() {
                        "styled"
                    } else {
                        "unstyled"
                    };
                    let label = format!("{} {source} {geometry} {control:?}", preset.kind());
                    record(
                        &mut text,
                        &label,
                        &arrow(preset, style, start, end, control),
                    );
                }
            }
        }
    }
    let [_, _, diagonal, short] = geometries();
    // One-factor sweeps from each preset's style.
    for &preset in Preset::ALL {
        for (geometry, start, end) in [diagonal, short] {
            for control in [Control::Default, Control::Above] {
                for (variant, style) in sweeps(&ArrowStyle::preset(preset)) {
                    let label = format!("{} {geometry} {control:?} {variant}", preset.kind());
                    record(
                        &mut text,
                        &label,
                        &arrow(preset, Some(style), start, end, control),
                    );
                }
            }
        }
    }
    // Every head, tail and shape together for the kinds with distinct shafts.
    for preset in [
        Preset::Forward,
        Preset::Equilibrium,
        Preset::Retro,
        Preset::Curved,
        Preset::Bent,
    ] {
        for &head in Head::ALL {
            for &tail in Head::ALL {
                for &shape in HeadShape::ALL {
                    let mut style = ArrowStyle::preset(preset);
                    style.head = head;
                    style.tail = tail;
                    style.shape = shape;
                    for (geometry, start, end) in [diagonal, short] {
                        for control in [Control::Default, Control::Below] {
                            let label = format!(
                                "{} {geometry} {control:?} head={head:?} tail={tail:?} shape={shape:?}",
                                preset.kind()
                            );
                            record(
                                &mut text,
                                &label,
                                &arrow(preset, Some(style.clone()), start, end, control),
                            );
                        }
                    }
                }
            }
        }
    }
    check_baseline("arrow_paths", &text);
}
