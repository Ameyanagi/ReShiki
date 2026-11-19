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

/// The reaction's condition texts: those linked to its arrow, and those set
/// along it, within a bond of it. A "+", a molecule's caption or coefficient,
/// a scheme title and the colour legend are reaction texts too, but outlining
/// them with the arrow would cover the reactants.
fn conditions(doc: &Document, reaction: &Reaction, arrow: &Arrow) -> Vec<u64> {
    let legend = reaction.colors.as_ref().and_then(|c| c.legend);
    let bond = doc.drawing_style.bond_length_world;
    let (dx, dy) = (arrow.end.x - arrow.start.x, arrow.end.y - arrow.start.y);
    let length = dx.hypot(dy);
    let (ux, uy) = (dx / length, dy / length);
    let along = |text: &reshiki::document::Annotation| {
        let (w, h) = text.size();
        let p = text.position;
        let corners = [
            (p.x, p.y),
            (p.x + w, p.y),
            (p.x, p.y + h),
            (p.x + w, p.y + h),
        ]
        .map(|(x, y)| (x - arrow.start.x, y - arrow.start.y));
        let t = corners.map(|(x, y)| x * ux + y * uy);
        let n = corners.map(|(x, y)| y * ux - x * uy);
        let (n_lo, n_hi) = (
            n.iter().copied().fold(f32::MAX, f32::min),
            n.iter().copied().fold(f32::MIN, f32::max),
        );
        let gap = if n_lo > 0. {
            n_lo
        } else if n_hi < 0. {
            -n_hi
        } else {
            0.
        };
        let pad = bond * 0.5;
        t.iter().all(|t| (-pad..=length + pad).contains(t)) && gap <= bond
    };
    doc.annotations
        .iter()
        .filter(|a| reaction.annotations.contains(&a.id) && Some(a.id) != legend)
        .filter(|a| {
            reshiki::arrow_labels::of(doc, arrow.id).any(|l| l.annotation == a.id) || along(a)
        })
        .map(|a| a.id)
        .collect()
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
    let conditions = arrow.map_or_else(Vec::new, |arrow| conditions(doc, reaction, arrow));
    if let Some(area) = bounds(doc, &index, &[], arrow, &conditions) {
        let (lo, hi) = outline(frame, camera, paper, area, overlay::ACCENT, 1.6);
        caption(
            frame,
            if conditions.is_empty() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use reshiki::document::Annotation;

    #[test]
    fn only_texts_along_the_arrow_are_conditions() {
        let mut doc = Document::default();
        let bond = doc.drawing_style.bond_length_world;
        let arrow = Arrow::new(
            9,
            World::new(0., 0.),
            World::new(3. * bond, 0.),
            reshiki::arrows::Preset::Forward,
            Default::default(),
        );
        // Above the arrow, a "+" and a caption beside it, a title far above,
        // and text linked to the arrow from afar.
        for (id, x, y, text) in [
            (1, 0.5 * bond, -0.8 * bond, "heat"),
            (2, -2. * bond, -0.2 * bond, " + "),
            (3, -3. * bond, 1.5 * bond, "benzene"),
            (4, 0.5 * bond, -3. * bond, "Scheme 1"),
            (5, 0.5 * bond, 4. * bond, "cat."),
        ] {
            doc.annotations.push(Annotation {
                id,
                position: World::new(x, y),
                text: text.into(),
                format: Default::default(),
            });
        }
        doc.arrow_labels.push(reshiki::arrow_labels::ArrowLabel {
            arrow: 9,
            annotation: 5,
            side: reshiki::arrow_labels::Side::Below,
            gap: None,
        });
        let mut reaction = Reaction::new(9);
        reaction.annotations = vec![1, 2, 3, 4, 5];
        assert_eq!(conditions(&doc, &reaction, &arrow), vec![1, 5]);
    }
}
