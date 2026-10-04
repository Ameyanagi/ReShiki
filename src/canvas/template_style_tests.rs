//! Exercise the actual hover draw branch with independent geometry references.
use super::*;
use iced::advanced::Renderer as _;
use iced::advanced::graphics::geometry::Renderer as _;
use iced::advanced::renderer::Headless;
use iced::widget::canvas::Program;
use reshiki::{
    document_styles::Preset,
    palette::{Color as Paint, Hue, Row},
    templates::{Anchor, Connection, LIBRARY, Template},
};

// The same test source runs on the baseline's Document input and the candidate's
// Template input. This adapter only borrows the original source; it never sizes it.
trait HoverSource<'a> {
    fn borrow(template: &'a Template) -> Self;
}
impl<'a> HoverSource<'a> for &'a Template {
    fn borrow(template: &'a Template) -> Self {
        template
    }
}
impl<'a> HoverSource<'a> for &'a Document {
    fn borrow(template: &'a Template) -> Self {
        &template.document
    }
}
fn source<'a, T: HoverSource<'a>>(template: &'a Template) -> T {
    T::borrow(template)
}

fn builtin() -> &'static Template {
    LIBRARY.iter().find(|t| t.name == "Cyclohexane").unwrap()
}

fn personal(scale: f32) -> Template {
    let mut template = builtin().clone();
    template.id = "custom:independent-hover-reference".into();
    // Authored Cyclohexane is centered on (0, 0). Do not invoke any placement or
    // transform helper to obtain the expected Nature coordinates.
    for atom in &mut template.document.atoms {
        atom.position.x *= scale;
        atom.position.y *= scale;
    }
    template
}

fn hover<'a>(host: &'a Document, template: &'a Template) -> MoleculeCanvas<'a> {
    let mut canvas = tests::chain_canvas(host, ChainMode::Straight);
    canvas.tool = Tool::Template;
    canvas.camera.zoom = 2.;
    canvas.template = Some((source(template), Anchor::Auto));
    canvas.template_connection = Connection::Connect;
    canvas
}

async fn renderer() -> Renderer {
    let backend = std::env::var("RESHIKI_PERF_RENDERER").ok();
    let renderer = <Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        backend.as_deref(),
    )
    .await
    .expect("Headless renderer required; this test must not silently skip");
    eprintln!("Template hover renderer: {}", renderer.name());
    renderer
}

fn render(
    renderer: &mut Renderer,
    canvas: &MoleculeCanvas<'_>,
    point: World,
    direction: Option<World>,
) -> Vec<u8> {
    let bounds = Rectangle::with_size(iced::Size::new(640., 400.));
    let position = canvas.camera.screen(direction.unwrap_or(point), bounds);
    let cursor = mouse::Cursor::Available(position);
    let mut state = State::default();
    let _ = canvas.update(
        &mut state,
        &Event::Mouse(mouse::Event::CursorMoved { position }),
        bounds,
        cursor,
    );
    assert_eq!(state.cursor, Some(position));
    if direction.is_some() {
        state.gesture = Some(Gesture::Ring {
            start: point,
            attached: true,
        });
    }
    renderer.reset(bounds);
    for geometry in canvas.draw(&state, renderer, &Theme::Light, bounds, cursor) {
        renderer.draw_geometry(geometry);
    }
    let mut pixels = Headless::screenshot(renderer, iced::Size::new(640, 400), 1., Color::WHITE);
    // Compare the entire drawing region, excluding only the bottom notice. A
    // frozen committed drawing has no "Preview" notice and should not need one.
    pixels.truncate(640 * 352 * 4);
    pixels
}

fn same_pixels(name: &str, actual: &[u8], expected: &[u8]) {
    assert_eq!(actual.len(), expected.len());
    let different = actual
        .as_chunks::<4>()
        .0
        .iter()
        .zip(expected.as_chunks::<4>().0)
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(different, 0, "{name}: drawing pixels differ");
}

fn ink_size(pixels: &[u8]) -> (usize, usize) {
    let mut lo = (usize::MAX, usize::MAX);
    let mut hi = (0, 0);
    for (index, pixel) in pixels.as_chunks::<4>().0.iter().enumerate() {
        if pixel[..3].iter().any(|value| *value < 245) {
            let (x, y) = (index % 640, index / 640);
            lo = (lo.0.min(x), lo.1.min(y));
            hi = (hi.0.max(x), hi.1.max(y));
        }
    }
    assert_ne!(lo.0, usize::MAX, "The actual canvas must draw a preview");
    (hi.0 - lo.0 + 1, hi.1 - lo.1 + 1)
}

#[tokio::test]
#[ignore = "Real canvas hover pixels; requires a headless renderer"]
async fn nature_builtin_hover_matches_independent_scaled_geometry() {
    let mut renderer = renderer().await;
    let nature = Document {
        drawing_style: Preset::Nature.style(),
        ..Default::default()
    };
    let ratio = nature.drawing_style.bond_length_world / Preset::Jacs.style().bond_length_world;
    let reference = personal(ratio);
    for bond in &reference.document.bonds {
        let a = reference.document.atom(bond.a).unwrap().position;
        let b = reference.document.atom(bond.b).unwrap().position;
        assert!(
            (a.distance(b) - nature.drawing_style.bond_length_world).abs() < 0.001,
            "Independent reference bonds must have the Nature nominal world length"
        );
    }
    let expected = render(
        &mut renderer,
        &hover(&nature, &reference),
        World::default(),
        None,
    );
    let actual = render(
        &mut renderer,
        &hover(&nature, builtin()),
        World::default(),
        None,
    );
    let size = ink_size(&expected);
    eprintln!(
        "Nature builtin/reference ink: {:?}/{size:?}",
        ink_size(&actual)
    );
    same_pixels(
        "Nature builtin hover vs manually scaled personal geometry",
        &actual,
        &expected,
    );
}

// Frozen baseline attachment geometry: the original 81ca821 capture's
// attached_nature_control-before_insert/after_insert native documents. These
// coordinates and edges are copied values, not another attachment algorithm.
const ATTACHED_ATOMS: [(f32, f32); 12] = [
    (-128.50342, -7.770718e-16),
    (-144.25171, 27.27684),
    (-175.74829, 27.27684),
    (-191.49658, -4.6342905e-15),
    (-175.74829, -27.27684),
    (-144.25171, -27.27684),
    (-49.976967, -27.889765),
    (-74.57167, -8.2140045),
    (-103.90872, -19.675758),
    (-108.65108, -50.81327),
    (-84.05638, -70.48903),
    (-54.71933, -59.027275),
];
const ATTACHED_BONDS: [(u64, u64, u8); 13] = [
    (1, 2, 1),
    (2, 3, 2),
    (3, 4, 1),
    (4, 5, 2),
    (5, 6, 1),
    (6, 1, 2),
    (7, 8, 1),
    (8, 9, 1),
    (9, 10, 1),
    (10, 11, 1),
    (11, 12, 1),
    (12, 7, 1),
    (1, 9, 1),
];

fn frozen_attachment(count: usize) -> Document {
    let mut doc = Document {
        drawing_style: Preset::Nature.style(),
        ..Default::default()
    };
    for &(x, y) in &ATTACHED_ATOMS[..count] {
        doc.add_atom("C", World::new(x, y));
    }
    for &(a, b, order) in &ATTACHED_BONDS {
        if a <= count as u64 && b <= count as u64 {
            doc.add_bond(a, b, order, "plain");
        }
    }
    doc
}

#[tokio::test]
#[ignore = "Real canvas control pixels; requires a headless renderer"]
async fn acs_personal_and_attached_hover_keep_legacy_geometry() {
    let mut renderer = renderer().await;
    let acs = Document::default();
    let saved = personal(1.);
    let acs_pixels = render(
        &mut renderer,
        &hover(&acs, builtin()),
        World::default(),
        None,
    );
    let saved_acs = render(&mut renderer, &hover(&acs, &saved), World::default(), None);
    same_pixels("ACS builtin/personal control", &acs_pixels, &saved_acs);
    let nature = Document {
        drawing_style: Preset::Nature.style(),
        ..Default::default()
    };
    let saved_nature = render(
        &mut renderer,
        &hover(&nature, &saved),
        World::default(),
        None,
    );
    let (acs_size, saved_size) = (ink_size(&acs_pixels), ink_size(&saved_nature));
    // Journal stroke widths differ slightly; a 25% geometry change is much larger.
    assert!(
        acs_size.0.abs_diff(saved_size.0) <= 2,
        "Personal width: {acs_size:?}/{saved_size:?}"
    );
    assert!(
        acs_size.1.abs_diff(saved_size.1) <= 2,
        "Personal height: {acs_size:?}/{saved_size:?}"
    );
    assert!(
        saved_size.0 > 160,
        "Saved ACS artwork was resized: {saved_size:?}"
    );

    let host = frozen_attachment(6);
    let mut expected = frozen_attachment(12);
    let tint = Paint::Palette(Hue::Teal, Row::Strong);
    for atom in &mut expected.atoms[6..] {
        atom.text_style.get_or_insert_with(Default::default).color = tint;
    }
    for bond in &mut expected.bonds[6..] {
        bond.color = tint;
    }
    let ids = [7, 8, 9, 10, 11, 12];
    let point = host.atoms[0].position;
    let direction = Some(point.offset(100., -80.));
    let mut actual_canvas = hover(&host, builtin());
    actual_canvas.camera.zoom = 1.;
    let actual = render(&mut renderer, &actual_canvas, point, direction);
    let mut expected_canvas = tests::chain_canvas(&expected, ChainMode::Straight);
    expected_canvas.tool = Tool::Template;
    expected_canvas.selected = &ids;
    let reference = render(&mut renderer, &expected_canvas, point, direction);
    assert!(
        ink_size(&reference).0 > 100,
        "Frozen attached drawing is visible"
    );
    same_pixels(
        "Attached hover vs frozen baseline geometry",
        &actual,
        &reference,
    );
}
