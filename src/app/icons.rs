use crate::canvas::Tool;
use iced::widget::canvas::{self, Geometry, Path, Stroke};
use iced::{Color, Point, Rectangle, Renderer, Theme, mouse};
mod arrows;
mod bonds;
mod chrome;
mod commands;
mod rings;
mod tools;

#[derive(Clone, Copy)]
pub(super) enum Icon {
    Sun,
    Moon,
    ColorTiles(bool),
    TextAlign(reshiki::typography::TextAlign),
    Tool(Tool),
    Ring(u8, bool),
    Arrow(reshiki::arrows::Preset),
    New,
    Open,
    Save,
    SaveAs,
    Assistant(bool),
    Check,
    Cleanup,
    Trash,
    Undo,
    Redo,
    Import,
    Export,
    Inspector,
    Keyboard,
    More,
    Lock(bool),
}
pub(super) struct Glyph(pub Icon, pub bool);
impl<Message> canvas::Program<Message> for Glyph {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut f = crate::canvas::layered::Frame::new(renderer, bounds.size()).with_theme(theme);
        self.paint(&mut f);
        f.finish()
    }
}
impl Glyph {
    pub(super) fn paint(&self, f: &mut crate::canvas::layered::Frame<'_>) {
        let ink = if self.1 {
            Color::from_rgb8(51, 62, 72)
        } else {
            Color::from_rgb8(187, 193, 199)
        };
        match self.0 {
            Icon::ColorTiles(soft) => chrome::color_tiles(f, ink, soft),
            Icon::Trash => commands::trash(f, ink),
            Icon::Sun => chrome::sun(f, ink),
            Icon::Moon => chrome::moon(f, ink),

            Icon::Keyboard => chrome::keyboard(f, ink),
            Icon::Tool(Tool::Chain(mode)) => tools::chain(f, ink, mode),
            Icon::Tool(Tool::Graphic(kind)) => tools::graphic(f, ink, kind),
            Icon::Tool(Tool::EditPoints) => tools::edit_points(f, ink),
            Icon::TextAlign(alignment) => chrome::text_align(f, ink, alignment),
            Icon::Tool(Tool::Select) => tools::select(f, ink),
            Icon::Tool(Tool::Lasso) => tools::lasso(f, ink),
            Icon::Tool(Tool::Tilt) => tools::tilt(f, ink),
            Icon::Tool(Tool::Bond(order)) => bonds::bond(f, ink, order),
            Icon::Tool(Tool::StyledBond(preset)) => bonds::styled_bond(f, ink, preset),
            Icon::Tool(Tool::Wedge) => bonds::wedge(f, ink),
            Icon::Tool(Tool::Hash) => bonds::hash(f, ink),
            Icon::Tool(Tool::Wavy) => bonds::wavy(f, ink),
            Icon::Tool(Tool::RingPreset(preset)) => rings::ring_preset(f, ink, preset),
            Icon::Ring(_, _) | Icon::Tool(Tool::Ring | Tool::Template) => {
                rings::ring(f, ink, self.0)
            }
            Icon::Arrow(_) | Icon::Tool(Tool::Arrow) => arrows::arrow(f, ink, self.0),
            Icon::Tool(Tool::Text) => tools::text(f, ink),
            Icon::Tool(Tool::Atom) => tools::atom(f, ink),
            Icon::Tool(Tool::Erase) => tools::erase(f, ink),
            Icon::New => commands::new_document(f, ink),
            Icon::Open => commands::open(f, ink),
            Icon::Save => commands::save(f, ink),
            Icon::SaveAs => commands::save_as(f, ink),
            Icon::Assistant(working) => commands::assistant(f, ink, working),
            Icon::Check => commands::check(f, ink),
            Icon::Cleanup => commands::cleanup(f, ink),
            Icon::Undo | Icon::Redo => commands::undo_redo(f, ink, self.0),
            Icon::Import | Icon::Export => commands::import_export(f, ink, self.0),
            Icon::Inspector => commands::inspector(f, ink),
            Icon::More => chrome::more(f, ink),
            Icon::Lock(locked) => chrome::lock(f, ink, locked),
        }
    }
}

fn line(f: &mut crate::canvas::layered::Frame<'_>, ink: Color, points: &[(f32, f32)]) {
    let path = Path::new(|p| {
        if let Some((x, y)) = points.first() {
            p.move_to(Point::new(*x, *y));
        }
        for (x, y) in points.iter().skip(1) {
            p.line_to(Point::new(*x, *y));
        }
    });
    f.stroke(&path, Stroke::default().with_width(1.6).with_color(ink));
}

fn polygon(f: &mut crate::canvas::layered::Frame<'_>, ink: Color, points: &[(f32, f32)]) {
    let path = Path::new(|p| {
        if let Some((x, y)) = points.first() {
            p.move_to(Point::new(*x, *y));
        }
        for (x, y) in points.iter().skip(1) {
            p.line_to(Point::new(*x, *y));
        }
        p.close();
    });
    f.fill(&path, ink);
}

#[cfg(test)]
mod pixel_tests;
