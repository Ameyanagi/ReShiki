//! A preview over a large drawing redraws the molecules it changes, not the drawing.
use crate::canvas::tests::chain_canvas;
use crate::canvas::{Camera, Gesture, State, Tool, World, render};
use iced::widget::canvas::Program;
use iced::{Point, Rectangle, Renderer, Theme, Vector, mouse};
use reshiki::chains::ChainMode;
use reshiki::document::Document;

fn gallery(copies: usize) -> Document {
    let original: Document = serde_json::from_str(include_str!(
        "../../../assets/examples/shortcut-examples.rsk"
    ))
    .unwrap();
    let mut doc = original.clone();
    let (lo, hi) = original.bounds();
    for copy in 1..copies {
        reshiki::editing::append(
            &mut doc,
            &original,
            World::new((hi.x - lo.x + 100.) * copy as f32, 0.),
        );
    }
    doc
}

/// The lowest end carbon and its neighbor.
fn chain_end(doc: &Document) -> (u64, u64) {
    let degree = |id| doc.bonds.iter().filter(|b| b.a == id || b.b == id).count();
    let end = doc
        .atoms
        .iter()
        .filter(|a| a.element == "C" && degree(a.id) == 1)
        .max_by(|a, b| a.position.y.total_cmp(&b.position.y))
        .unwrap()
        .id;
    let neighbor = doc
        .bonds
        .iter()
        .find_map(|b| match (b.a == end, b.b == end) {
            (true, _) => Some(b.b),
            (_, true) => Some(b.a),
            _ => None,
        })
        .unwrap();
    (end, neighbor)
}

/// The preview cases the canvas benchmark measures, at `hover` (paper below
/// the gallery) and from the chain end `start`.
fn cases() -> [&'static str; 4] {
    ["ring_preset", "template", "bond", "partial_move"]
}

fn set_case(
    canvas: &mut crate::canvas::MoleculeCanvas<'_>,
    state: &mut State,
    case: &str,
    (end, neighbor): (u64, u64),
    moving: &'static [u64],
) {
    let end_position = canvas.doc.atom(end).unwrap().position;
    state.gesture = None;
    canvas.template = None;
    canvas.selected = &[];
    match case {
        "ring_preset" => canvas.tool = Tool::RingPreset(reshiki::rings::Preset::Benzene),
        "template" => {
            canvas.tool = Tool::Template;
            canvas.template = Some((
                reshiki::templates::LIBRARY
                    .iter()
                    .find(|t| t.name == "Cyclohexane")
                    .unwrap(),
                reshiki::templates::Anchor::Auto,
            ));
        }
        "bond" => {
            canvas.tool = Tool::Bond(1);
            state.gesture = Some(Gesture::Draw {
                start: end_position,
                pressed: end_position,
                id: Some(end),
                text: None,
            });
        }
        _ => {
            canvas.tool = Tool::Select;
            canvas.selected = moving;
            state.gesture = Some(Gesture::Move {
                start: end_position,
                ids: vec![end, neighbor],
                clicked: vec![end, neighbor],
            });
        }
    }
}

/// The audit's P1: every preview frame rebuilt and redrew the whole drawing.
#[tokio::test]
#[ignore = "Preview geometry reuse; requires a headless renderer"]
async fn preview_frames_draw_only_the_changed_molecules() {
    let renderer = <Renderer as iced::advanced::renderer::Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .expect("Headless renderer");
    let doc = gallery(4);
    let whole = reshiki::scene::primitives(&doc).len();
    let (end, neighbor) = chain_end(&doc);
    let moving: &'static [u64] = Box::leak(Box::new([end, neighbor]));
    let (_, hi) = doc.bounds();
    let end_position = doc.atom(end).unwrap().position;
    let bounds = Rectangle::with_size(iced::Size::new(1066., 570.));
    let mut canvas = chain_canvas(&doc, ChainMode::Straight);
    canvas.camera = Camera {
        center: World::new(end_position.x, hi.y),
        zoom: 0.5,
    };
    let length = doc.drawing_style.bond_length_world;
    let hover = canvas
        .camera
        .screen(World::new(end_position.x, hi.y + 4. * length), bounds);
    let start = canvas.camera.screen(end_position, bounds);
    let mut redrawn = Vec::new();
    for case in cases() {
        let mut state = State::default();
        set_case(&mut canvas, &mut state, case, (end, neighbor), moving);
        let at = if matches!(case, "bond" | "partial_move") {
            start + Vector::new(30., -10.)
        } else {
            hover
        };
        let mut drawn = |point: Point| {
            state.cursor = Some(point);
            render::DRAWN_PRIMITIVES.with(|n| n.set(0));
            let _ = canvas.draw(
                &state,
                &renderer,
                &Theme::Light,
                bounds,
                mouse::Cursor::Available(point),
            );
            render::DRAWN_PRIMITIVES.with(std::cell::Cell::get)
        };
        drawn(at);
        let next = drawn(at + Vector::new(3., 1.));
        println!("COUNT,{case},{next},{whole}");
        redrawn.push((case, next));
    }
    assert!(
        redrawn.iter().all(|(_, next)| next * 20 < whole),
        "of the drawing's {whole} primitives, previews redrew {redrawn:?}"
    );
}

/// Drawing the changed molecules over the retained rest gives exactly the
/// pixels of drawing the whole preview, wherever the pointer goes.
#[tokio::test]
#[ignore = "Pixel regression check; requires a headless renderer"]
async fn preview_fragments_match_whole_previews() {
    use iced::advanced::Renderer as _;
    use iced::advanced::graphics::geometry::Renderer as _;
    use iced::advanced::renderer::Headless;
    let backend = std::env::var("RESHIKI_PERF_RENDERER").ok();
    let mut renderer = <Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        backend.as_deref(),
    )
    .await
    .expect("Headless renderer");
    let doc = gallery(1);
    let ends = chain_end(&doc);
    let moving: &'static [u64] = Box::leak(Box::new([ends.0, ends.1]));
    let size = iced::Size::new(800, 500);
    let bounds = Rectangle::with_size(iced::Size::new(800., 500.));
    let (_, hi) = doc.bounds();
    let end = doc.atom(ends.0).unwrap().position;
    let mut shot = |canvas: &crate::canvas::MoleculeCanvas<'_>, state: &State, split: bool| {
        super::fragment::SPLIT.with(|s| s.set(split));
        render::DRAWN_PRIMITIVES.with(|n| n.set(0));
        renderer.reset(bounds);
        let cursor = mouse::Cursor::Available(state.cursor.unwrap());
        for layer in canvas.draw(state, &renderer, &Theme::Light, bounds, cursor) {
            renderer.draw_geometry(layer);
        }
        let drawn = render::DRAWN_PRIMITIVES.with(std::cell::Cell::get);
        super::fragment::SPLIT.with(|s| s.set(true));
        (
            Headless::screenshot(&mut renderer, size, 1., iced::Color::WHITE),
            drawn,
        )
    };
    let (mut frames, mut split) = (0, 0);
    for zoom in [0.5, 1.25] {
        for center in [World::new(378., 294.), World::new(end.x, hi.y)] {
            for case in cases() {
                let mut canvas = chain_canvas(&doc, ChainMode::Straight);
                canvas.camera = Camera { center, zoom };
                let mut state = State::default();
                set_case(&mut canvas, &mut state, case, ends, moving);
                for x in 0..6 {
                    for y in 0..4 {
                        let point = Point::new(60. + x as f32 * 135., 50. + y as f32 * 130.);
                        state.cursor = Some(point);
                        let (fragment, partial) = shot(&canvas, &state, true);
                        let (whole, full) = shot(&canvas, &state, false);
                        assert!(
                            fragment == whole,
                            "{case} at {point:?}, zoom {zoom}, center {center:?}"
                        );
                        frames += 1;
                        split += usize::from(partial < full);
                    }
                }
            }
        }
    }
    println!("SPLIT,{split} of {frames} frames");
    assert!(split * 2 > frames, "only {split} of {frames} frames split");
}
