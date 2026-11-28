//! Opt-in rendering fixture, not a scientific transition-state assertion.
use super::*;

#[tokio::test]
#[ignore = "Opt-in editable scheme canvas/native evidence; sets RESHIKI_SCHEME_EVIDENCE"]
async fn manual_editable_scheme_evidence() {
    use crate::canvas::{Camera, MoleculeCanvas, PreviewStyles};
    use iced::advanced::{Renderer as _, graphics::geometry::Renderer as _, renderer::Headless};
    use iced::{Rectangle, Renderer, Size, Theme, mouse, widget::canvas::Program};
    let directory = std::path::PathBuf::from(
        std::env::var_os("RESHIKI_SCHEME_EVIDENCE").expect("RESHIKI_SCHEME_EVIDENCE"),
    );
    std::fs::create_dir_all(&directory).unwrap();
    // Exact coordinates from files/tests.rs::BUTANOL_ATOMS, the existing
    // calculation-output stereo regression; its z mirror is the other enantiomer.
    let xyz = "15\nS-2-butanol import fixture\n\
C 1.25803 -0.24884 -1.03575\nC 0.71249 0.40000 0.22840\nO 1.69559 0.24011 1.25103\n\
C -0.60148 -0.22109 0.71030\nC -1.75492 -0.03925 -0.26243\nH 1.39531 -1.32664 -0.89494\n\
H 2.24368 0.16273 -1.27992\nH 0.59910 -0.08740 -1.89329\nH 0.58481 1.47497 0.05823\n\
H 1.36811 0.69385 2.04631\nH -0.45617 -1.28893 0.91627\nH -0.87532 0.22784 1.67349\n\
H -1.90477 1.01849 -0.50054\nH -1.58306 -0.58423 -1.19518\nH -2.68140 -0.42161 0.17801\n";
    let file = reshiki::qcread::read("butanol.xyz", xyz.as_bytes()).unwrap();
    let first = &file.jobs[0].steps[0].frame;
    let mut mirror = first.clone();
    for point in &mut mirror.positions_angstrom {
        point[2] = -point[2];
    }
    let frames = [first.clone(), mirror, first.clone()];
    std::fs::write(
        directory.join("input-frames.json"),
        serde_json::to_vec_pretty(&frames).unwrap(),
    )
    .unwrap();
    std::fs::write(directory.join("provenance.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "purpose": "Editable scheme rendering fixture; no scientific TS/reaction claim",
        "geometry_source": "src/app/files/tests.rs::BUTANOL_ATOMS (S-2-butanol), plus its z mirror",
        "connections": "Mock confirmed-segment/gap records exercise the placement UI only",
        "energies": "Test placeholders; no calculated energies attached to saved drawings",
        "renderer": "Production MoleculeCanvas::preview -> Program::draw -> Iced headless renderer",
        "test": "app::xtb::ts::scheme::drawing::tests::evidence::manual_editable_scheme_evidence"
    })).unwrap()).unwrap();
    let mut source = Document::default();
    caption(&mut source, "Drawing example · 2-butanol mirror geometries\nArrows and gaps use simulated search results".into(), Point::default());
    let mut renderer = <Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .expect("Headless renderer");
    let size = Size::new(1400, 720);
    let bounds = Rectangle::with_size(Size::new(size.width as f32, size.height as f32));
    let graphic_style = reshiki::graphics::GraphicStyle::default();
    let arrow_style = reshiki::arrows::ArrowStyle::default();
    for gaps in [false, true] {
        let mut discovered = path(3);
        for (minimum, frame) in discovered.minima.iter_mut().zip(&frames) {
            minimum.frame = Arc::new(frame.clone());
        }
        if gaps {
            discovered.segments.pop();
            discovered.gaps.push(DiscoveredGap {
                reactant: 1,
                product: 2,
                reason: "Unresolved UI fixture connection".into(),
            });
        }
        let report = report(discovered, 1);
        let placed = build(&source, &report).unwrap();
        assert!(placed.warnings.is_empty(), "{:?}", placed.warnings);
        let repeated = build(&placed.document, &report).unwrap();
        for (suffix, doc) in [
            (if gaps { "gap" } else { "connected" }, placed.document),
            (
                if gaps {
                    "gap-repeat"
                } else {
                    "connected-repeat"
                },
                repeated.document,
            ),
        ] {
            let (lo, hi) = reshiki::scene::selection_bounds(&doc, &doc.all_ids()).unwrap();
            let camera = Camera {
                center: lo.lerp(hi, 0.5),
                zoom: ((bounds.width - 80.) / (hi.x - lo.x))
                    .min((bounds.height - 80.) / (hi.y - lo.y))
                    .min(1.8),
            };
            let canvas = MoleculeCanvas::preview(
                &doc,
                &[],
                camera,
                PreviewStyles {
                    bond_drawing: Default::default(),
                    chain_drawing: Default::default(),
                    arrow_preset: Default::default(),
                    arrow_style: &arrow_style,
                    orbital_phase: Default::default(),
                    graphic_style: &graphic_style,
                    bracket_sides: reshiki::graphics::BracketSides::Both,
                },
            );
            renderer.reset(bounds);
            for layer in canvas.draw(
                &Default::default(),
                &renderer,
                &Theme::Light,
                bounds,
                mouse::Cursor::Unavailable,
            ) {
                renderer.draw_geometry(layer);
            }
            let pixels = Headless::screenshot(&mut renderer, size, 1., iced::Color::WHITE);
            image::RgbaImage::from_raw(size.width, size.height, pixels)
                .unwrap()
                .save(directory.join(format!("editable-{suffix}.png")))
                .unwrap();
            std::fs::write(
                directory.join(format!("editable-{suffix}.rsk")),
                serde_json::to_vec_pretty(&doc).unwrap(),
            )
            .unwrap();
        }
    }
}
