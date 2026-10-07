//! Capture every icon glyph once, then compare candidate pixels against that
//! baseline on the same machine, backend and fonts.
use super::*;
use iced::advanced::Renderer as _;
use iced::advanced::graphics::geometry::Renderer as _;
use iced::advanced::renderer::Headless;
use iced::widget::canvas::Program;
use iced::{Size, Vector};
use std::collections::{BTreeMap, BTreeSet};

/// Family of every icon. Adding a variant breaks this match; extend
/// all_icons() and expected_counts() too.
fn family(icon: Icon) -> &'static str {
    match icon {
        Icon::Sun => "sun",
        Icon::Moon => "moon",
        Icon::ColorTiles(_) => "color-tiles",
        Icon::TextAlign(_) => "text-align",
        Icon::Tool(tool) => tool_family(tool),
        Icon::Ring(..) => "ring",
        Icon::Arrow(_) => "arrow",
        Icon::New => "new",
        Icon::Open => "open",
        Icon::Save => "save",
        Icon::SaveAs => "save-as",
        Icon::Assistant(_) => "assistant",
        Icon::Check => "check",
        Icon::Cleanup => "cleanup",
        Icon::Trash => "trash",
        Icon::Undo => "undo",
        Icon::Redo => "redo",
        Icon::Import => "import",
        Icon::Export => "export",
        Icon::Inspector => "inspector",
        Icon::Keyboard => "keyboard",
        Icon::More => "more",
        Icon::Lock(_) => "lock",
    }
}

/// Family of every tool. Adding a variant breaks this match; extend
/// all_icons() and expected_counts() too.
fn tool_family(tool: Tool) -> &'static str {
    match tool {
        Tool::Select => "tool-select",
        Tool::Lasso => "tool-lasso",
        Tool::Tilt => "tool-tilt",
        Tool::Chain(_) => "tool-chain",
        Tool::Bond(_) => "tool-bond",
        Tool::StyledBond(_) => "tool-styled-bond",
        Tool::Wedge => "tool-wedge",
        Tool::Hash => "tool-hash",
        Tool::Wavy => "tool-wavy",
        Tool::Atom => "tool-atom",
        Tool::Ring => "tool-ring",
        Tool::RingPreset(_) => "tool-ring-preset",
        Tool::Template => "tool-template",
        Tool::Arrow => "tool-arrow",
        Tool::Text => "tool-text",
        Tool::Erase => "tool-erase",
        Tool::Graphic(_) => "tool-graphic",
        Tool::EditPoints => "tool-edit-points",
    }
}

fn all_icons() -> Vec<(String, Icon)> {
    let mut icons: Vec<(String, Icon)> = [
        Icon::Sun,
        Icon::Moon,
        Icon::New,
        Icon::Open,
        Icon::Save,
        Icon::SaveAs,
        Icon::Check,
        Icon::Cleanup,
        Icon::Trash,
        Icon::Undo,
        Icon::Redo,
        Icon::Import,
        Icon::Export,
        Icon::Inspector,
        Icon::Keyboard,
        Icon::More,
    ]
    .into_iter()
    .map(|icon| (family(icon).to_string(), icon))
    .collect();
    icons.extend([false, true].map(|soft| (format!("color-tiles-{soft}"), Icon::ColorTiles(soft))));
    icons.extend(
        [
            reshiki::typography::TextAlign::Left,
            reshiki::typography::TextAlign::Center,
            reshiki::typography::TextAlign::Right,
            reshiki::typography::TextAlign::Justified,
        ]
        .map(|align| (format!("text-align-{align:?}"), Icon::TextAlign(align))),
    );
    icons.extend(
        [false, true].map(|working| (format!("assistant-{working}"), Icon::Assistant(working))),
    );
    icons.extend([false, true].map(|locked| (format!("lock-{locked}"), Icon::Lock(locked))));
    for size in 3..=8 {
        for aromatic in [false, true] {
            icons.push((
                format!("ring-{size}-{aromatic}"),
                Icon::Ring(size, aromatic),
            ));
        }
    }
    icons.extend(
        reshiki::arrows::Preset::ALL
            .iter()
            .map(|&preset| (format!("arrow-{preset:?}"), Icon::Arrow(preset))),
    );
    let mut tools = vec![
        Tool::Select,
        Tool::Lasso,
        Tool::Tilt,
        Tool::Wedge,
        Tool::Hash,
        Tool::Wavy,
        Tool::Atom,
        Tool::Ring,
        Tool::Template,
        Tool::Arrow,
        Tool::Text,
        Tool::Erase,
        Tool::EditPoints,
    ];
    tools.extend(
        [
            reshiki::chains::ChainMode::Straight,
            reshiki::chains::ChainMode::Snaking,
        ]
        .map(Tool::Chain),
    );
    tools.extend([1, 2, 3].map(Tool::Bond));
    tools.extend(reshiki::bonds::BondPreset::ALL.map(Tool::StyledBond));
    tools.extend(
        reshiki::rings::Preset::ALL
            .iter()
            .map(|&preset| Tool::RingPreset(preset)),
    );
    tools.extend(reshiki::graphics::GraphicKind::DRAWABLE.map(Tool::Graphic));
    tools.extend(
        reshiki::scientific::SymbolKind::ALL
            .iter()
            .map(|&symbol| Tool::Graphic(reshiki::graphics::GraphicKind::Symbol(symbol))),
    );
    tools.extend(
        reshiki::scientific::OrbitalKind::ALL
            .iter()
            .map(|&orbital| Tool::Graphic(reshiki::graphics::GraphicKind::Orbital(orbital))),
    );
    tools.extend(
        [
            reshiki::graphics::GraphicKind::Picture,
            reshiki::graphics::GraphicKind::Path,
        ]
        .map(Tool::Graphic),
    );
    icons.extend(tools.into_iter().map(|tool| {
        let debug = format!("{tool:?}");
        let parts: Vec<&str> = debug
            .split(|c: char| !c.is_alphanumeric())
            .filter(|part| !part.is_empty())
            .collect();
        (format!("tool-{}", parts.join("_")), Icon::Tool(tool))
    }));
    icons
}

fn expected_counts() -> BTreeMap<&'static str, usize> {
    let mut counts: BTreeMap<&'static str, usize> = [
        "sun",
        "moon",
        "new",
        "open",
        "save",
        "save-as",
        "check",
        "cleanup",
        "trash",
        "undo",
        "redo",
        "import",
        "export",
        "inspector",
        "keyboard",
        "more",
        "tool-select",
        "tool-lasso",
        "tool-tilt",
        "tool-wedge",
        "tool-hash",
        "tool-wavy",
        "tool-atom",
        "tool-ring",
        "tool-template",
        "tool-arrow",
        "tool-text",
        "tool-erase",
        "tool-edit-points",
    ]
    .into_iter()
    .map(|family| (family, 1))
    .collect();
    counts.extend([
        ("color-tiles", 2),
        ("text-align", 4),
        ("assistant", 2),
        ("lock", 2),
        ("ring", 12),
        ("arrow", reshiki::arrows::Preset::ALL.len()),
        ("tool-chain", 2),
        ("tool-bond", 3),
        ("tool-styled-bond", reshiki::bonds::BondPreset::ALL.len()),
        ("tool-ring-preset", reshiki::rings::Preset::ALL.len()),
        (
            "tool-graphic",
            reshiki::graphics::GraphicKind::DRAWABLE.len()
                + reshiki::scientific::SymbolKind::ALL.len()
                + reshiki::scientific::OrbitalKind::ALL.len()
                + 2,
        ),
    ]);
    counts
}

#[test]
fn icon_fixtures_cover_every_variant() {
    let icons = all_icons();
    let labels: BTreeSet<&str> = icons.iter().map(|(label, _)| label.as_str()).collect();
    assert_eq!(labels.len(), icons.len(), "icon labels must be unique");
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for (_, icon) in &icons {
        *counts.entry(family(*icon)).or_insert(0) += 1;
    }
    assert_eq!(counts, expected_counts());
    assert_eq!(icons.len(), 125);
}

fn render(
    renderer: &mut Renderer,
    logical: f32,
    physical: u32,
    scale: f32,
    theme: &Theme,
    draw: impl Fn(&Renderer) -> Vec<Geometry>,
) -> Vec<u8> {
    renderer.reset(Rectangle::with_size(Size::new(logical, logical)));
    let geometries = draw(&*renderer);
    for geometry in geometries {
        renderer.draw_geometry(geometry);
    }
    Headless::screenshot(
        renderer,
        Size::new(physical, physical),
        scale,
        theme.palette().background,
    )
}

fn check_icon_pixels(
    dir: &std::path::Path,
    name: &str,
    width: usize,
    pixels: &[u8],
    capture: bool,
) -> Option<String> {
    let path = dir.join(format!("{name}.rgba"));
    let mismatch = if capture {
        std::fs::write(&path, pixels)
            .unwrap_or_else(|error| panic!("Write baseline {}: {error}", path.display()));
        None
    } else {
        let expected = std::fs::read(&path)
            .unwrap_or_else(|error| panic!("Read baseline {}: {error}", path.display()));
        if pixels.len() == expected.len() {
            let different = pixels.iter().zip(&expected).filter(|(a, b)| a != b).count();
            pixels
                .iter()
                .zip(&expected)
                .position(|(a, b)| a != b)
                .map(|i| {
                    let pixel = i / 4;
                    let at = pixel * 4..pixel * 4 + 4;
                    format!(
                        "{name}: {different} different channels; first pixel ({}, {}), actual {:?}, expected {:?}",
                        pixel % width,
                        pixel / width,
                        &pixels[at.clone()],
                        &expected[at]
                    )
                })
        } else {
            Some(format!(
                "{name}: {} bytes, expected {}",
                pixels.len(),
                expected.len()
            ))
        }
    };
    println!(
        "PIXEL,{name},{} bytes,{}",
        pixels.len(),
        if capture {
            "captured"
        } else if mismatch.is_some() {
            "MISMATCH"
        } else {
            "matched"
        }
    );
    mismatch
}

#[tokio::test]
#[ignore = "Icon baseline/candidate pixels; requires a headless renderer and artifact directory"]
async fn icon_glyphs_match_captured_baseline() {
    let dir = std::path::PathBuf::from(
        std::env::var("RESHIKI_CANVAS_PIXELS")
            .expect("Set RESHIKI_CANVAS_PIXELS to the baseline artifact directory"),
    )
    .join("icons");
    let capture = std::env::var("RESHIKI_CANVAS_CAPTURE_BASELINE").as_deref() == Ok("1");
    let backend = std::env::var("RESHIKI_PERF_RENDERER").ok();
    let mut renderer = <Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        backend.as_deref(),
    )
    .await
    .expect("Headless renderer");
    let icons = all_icons();
    let labels: Vec<&str> = icons.iter().map(|(label, _)| label.as_str()).collect();
    let metadata = serde_json::json!({
        "renderer": renderer.name(),
        "backend": backend,
        "font": reshiki::style::ui_font_family(),
        "labels": labels,
        "themes": ["light", "dark"],
        "blank": ["tool-Graphic_Path"],
        "configs": [
            {"name": "glyph", "logical": 24, "physical": 24, "scale": 1, "inks": ["enabled", "disabled"]},
            {"name": "button", "logical": 36, "physical": 72, "scale": 2, "offset": 6, "inks": ["enabled"]},
            {"name": "lock20", "logical": 20, "physical": 20, "scale": 1, "inks": ["enabled"], "labels": ["lock-false", "lock-true"]},
        ],
    });
    if capture {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("metadata.json"),
            serde_json::to_vec_pretty(&metadata).unwrap(),
        )
        .unwrap();
    } else {
        let expected: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("metadata.json")).unwrap()).unwrap();
        assert_eq!(
            metadata, expected,
            "Use the same renderer, fonts and fixtures as the baseline"
        );
    }
    let configs = [
        ("glyph", 24., 24, 1., vec![true, false]),
        ("button", 36., 72, 2., vec![true]),
        ("lock20", 20., 20, 1., vec![true]),
    ];
    let mut images = 0;
    let mut mismatches = Vec::new();
    for (config, logical, physical, scale, inks) in configs {
        for (theme_name, theme) in [("light", Theme::Light), ("dark", Theme::Dark)] {
            for (label, icon) in &icons {
                if config == "lock20" && !matches!(icon, Icon::Lock(_)) {
                    continue;
                }
                let expect_blank = matches!(
                    icon,
                    Icon::Tool(Tool::Graphic(reshiki::graphics::GraphicKind::Path))
                );
                for &enabled in &inks {
                    let ink = if enabled { "enabled" } else { "disabled" };
                    let glyph = Glyph(*icon, enabled);
                    let draw = |r: &Renderer| {
                        if config == "button" {
                            let mut frame =
                                crate::canvas::layered::Frame::new(r, Size::new(36., 36.))
                                    .with_theme(&theme);
                            frame.translate(Vector::new(6., 6.));
                            glyph.paint(&mut frame);
                            frame.finish()
                        } else {
                            <Glyph as Program<()>>::draw(
                                &glyph,
                                &(),
                                r,
                                &theme,
                                Rectangle::with_size(Size::new(logical, logical)),
                                mouse::Cursor::Unavailable,
                            )
                        }
                    };
                    let name = format!("{config}-{theme_name}-{ink}-{label}");
                    let pixels = render(&mut renderer, logical, physical, scale, &theme, draw);
                    let warm = render(&mut renderer, logical, physical, scale, &theme, draw);
                    assert!(pixels == warm, "{name}: cold/warm rendering differs");
                    let (rgba, _) = pixels.as_chunks::<4>();
                    let blank = rgba.iter().all(|p| p[..] == pixels[..4]);
                    assert_eq!(blank, expect_blank, "{name}: blank output");
                    mismatches.extend(check_icon_pixels(
                        &dir,
                        &name,
                        physical as usize,
                        &pixels,
                        capture,
                    ));
                    images += 1;
                }
            }
        }
    }
    assert_eq!(images, 754);
    assert!(
        mismatches.is_empty(),
        "{} icon images differ:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}
