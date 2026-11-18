//! Editing marks for object purpose and the reaction preview. They are drawn
//! on the canvas only, never in exports or copied images.
use super::{Camera, World, layered, overlay};
use iced::widget::canvas::{self, LineDash, Path, Stroke};
use iced::{Color, Point, Rectangle, Size};
use reshiki::document::{Arrow, Atom, Document};
use reshiki::purpose::Purpose;
use reshiki::reactions::{Reaction, Role};
use std::collections::HashMap;

/// Muted ink for the light and the dark canvas.
const QUIET: Color = Color::from_rgba8(118, 126, 124, 0.85);
const QUIET_DARK: Color = Color::from_rgba8(176, 186, 184, 0.9);

/// The drawing's visible atoms by ID, built once per redraw.
fn visible(doc: &Document) -> HashMap<u64, &Atom> {
    doc.atoms
        .iter()
        .filter(|a| doc.atom_visible(a.id))
        .map(|a| (a.id, a))
        .collect()
}

/// The world bounds of these atoms, arrow and texts, padded by half a bond.
fn bounds(
    doc: &Document,
    index: &HashMap<u64, &Atom>,
    atoms: &[u64],
    arrow: Option<&Arrow>,
    texts: &[u64],
) -> Option<(World, World)> {
    let mut points: Vec<World> = atoms
        .iter()
        .filter_map(|id| index.get(id).map(|a| a.position))
        .collect();
    if let Some(arrow) = arrow {
        let (lo, hi) = arrow.bounds();
        points.extend([lo, hi]);
    }
    for text in doc.annotations.iter().filter(|a| texts.contains(&a.id)) {
        let (w, h) = text.size();
        points.extend([text.position, text.position.offset(w, h)]);
    }
    let first = *points.first()?;
    let (lo, hi) = points.iter().fold((first, first), |(lo, hi), p| {
        (
            World::new(lo.x.min(p.x), lo.y.min(p.y)),
            World::new(hi.x.max(p.x), hi.y.max(p.y)),
        )
    });
    let pad = doc.drawing_style.bond_length_world * 0.45;
    Some((lo.offset(-pad, -pad), hi.offset(pad, pad)))
}

fn outline(
    frame: &mut layered::Frame<'_>,
    camera: Camera,
    paper: Rectangle,
    (lo, hi): (World, World),
    color: Color,
    width: f32,
) -> (Point, Point) {
    let (lo, hi) = (camera.screen(lo, paper), camera.screen(hi, paper));
    let path = Path::rounded_rectangle(lo, Size::new(hi.x - lo.x, hi.y - lo.y), 6.0.into());
    frame.stroke(
        &path,
        Stroke {
            line_dash: LineDash {
                segments: &[6., 4.],
                offset: 0,
            },
            ..Stroke::default().with_width(width).with_color(color)
        },
    );
    (lo, hi)
}

fn caption(frame: &mut layered::Frame<'_>, text: &str, at: Point, color: Color, centered: bool) {
    frame.fill_text(canvas::Text {
        content: text.into(),
        position: at,
        color,
        size: 11.into(),
        align_x: if centered {
            iced::advanced::text::Alignment::Center
        } else {
            iced::advanced::text::Alignment::Left
        },
        ..Default::default()
    });
}

/// A dashed outline and a small tag on each Reference and Illustration
/// molecule and arrow.
pub(super) fn draw_cues(
    doc: &Document,
    frame: &mut layered::Frame<'_>,
    camera: Camera,
    paper: Rectangle,
) {
    if !reshiki::purpose::used(doc) {
        return;
    }
    let index = visible(doc);
    let ids: Vec<_> = doc
        .atoms
        .iter()
        .filter(|a| !a.purpose.is_chemistry())
        .map(|a| a.id)
        .collect();
    let purposes: HashMap<_, _> = doc.atoms.iter().map(|a| (a.id, a.purpose)).collect();
    // Purpose from any member: the first may be hidden in an abbreviation.
    let molecules = reshiki::reactions::molecules(doc, &ids)
        .into_iter()
        .filter_map(|atoms| {
            Some((
                *purposes.get(atoms.first()?)?,
                bounds(doc, &index, &atoms, None, &[])?,
                false,
            ))
        });
    let arrows = doc
        .arrows
        .iter()
        .filter(|a| !a.purpose.is_chemistry())
        .filter_map(|a| Some((a.purpose, bounds(doc, &index, &[], Some(a), &[])?, true)));
    let quiet = if doc.canvas_theme.is_light() {
        QUIET
    } else {
        QUIET_DARK
    };
    for (purpose, area, arrow) in molecules.chain(arrows) {
        let (lo, hi) = outline(frame, camera, paper, area, quiet, 1.);
        let tag = match purpose {
            Purpose::Reference => "Reference",
            _ => "Illustration",
        };
        // Text sits above and below an arrow, so its tag goes past the end.
        let at = if arrow {
            Point::new(hi.x + 4., (lo.y + hi.y) / 2. - 7.)
        } else {
            Point::new(lo.x + 4., lo.y - 14.)
        };
        caption(frame, tag, at, quiet, false);
    }
}

/// Outline what takes part in a reaction, with each part's role.
pub(super) fn draw_preview(
    doc: &Document,
    reaction: &Reaction,
    frame: &mut layered::Frame<'_>,
    camera: Camera,
    paper: Rectangle,
) {
    let index = visible(doc);
    for role in Role::ALL {
        let name = match role {
            Role::Reactant => "reactant",
            Role::Product => "product",
            Role::Agent => "reagent",
        };
        for participant in reaction.participants(role) {
            if let Some(area) = bounds(doc, &index, &participant.atoms, None, &[]) {
                let (lo, hi) = outline(frame, camera, paper, area, overlay::ACCENT, 1.6);
                caption(
                    frame,
                    name,
                    Point::new((lo.x + hi.x) / 2., hi.y + 4.),
                    overlay::ACCENT_STRONG,
                    true,
                );
            }
        }
    }
    let arrow = doc.arrows.iter().find(|a| a.id == reaction.arrow);
    if let Some(area) = bounds(doc, &index, &[], arrow, &reaction.annotations) {
        let (lo, hi) = outline(frame, camera, paper, area, overlay::ACCENT, 1.6);
        caption(
            frame,
            if reaction.annotations.is_empty() {
                "arrow"
            } else {
                "conditions"
            },
            Point::new((lo.x + hi.x) / 2., hi.y + 4.),
            overlay::ACCENT_STRONG,
            true,
        );
    }
}
