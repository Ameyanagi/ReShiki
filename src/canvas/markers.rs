//! Selection decorations in world coordinates, retained with the current scene.
use super::{Camera, Document, Point, Rectangle, World, layered, rgb};
use iced::widget::canvas::{Path, Stroke};
use iced::{Color, Size};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Default, PartialEq)]
pub(super) struct Markers {
    multiple: bool,
    bonds: Vec<(World, World)>,
    atoms: Vec<World>,
    captions: Vec<(World, Size)>,
}

impl Markers {
    pub fn new(doc: &Document, ids: &[u64]) -> Self {
        Self::build(doc, ids, true)
    }

    pub fn hover(doc: &Document, ids: &[u64]) -> Self {
        Self::build(doc, ids, false)
    }

    fn build(doc: &Document, ids: &[u64], captions: bool) -> Self {
        if ids.is_empty() {
            return Self::default();
        }
        let selected: HashSet<_> = ids.iter().copied().collect();
        let atoms: HashMap<_, _> = doc.atoms.iter().map(|atom| (atom.id, atom)).collect();
        let hidden: HashSet<_> = doc
            .abbreviations
            .iter()
            .flat_map(|group| group.members.iter().filter(|id| **id != group.anchor))
            .copied()
            .collect();
        Self {
            multiple: ids.len() > 1,
            bonds: doc
                .bonds
                .iter()
                .filter(|bond| selected.contains(&bond.a) && selected.contains(&bond.b))
                .filter(|bond| doc.bond_visible(bond.a, bond.b))
                .filter_map(|bond| {
                    Some((atoms.get(&bond.a)?.position, atoms.get(&bond.b)?.position))
                })
                .collect(),
            // Preserve selection order, including its behavior for repeated IDs.
            atoms: ids
                .iter()
                .filter(|id| !hidden.contains(id))
                .filter_map(|id| atoms.get(id).map(|atom| atom.position))
                .collect(),
            captions: doc
                .annotations
                .iter()
                .filter(|caption| captions && selected.contains(&caption.id))
                .map(|caption| {
                    let (width, height) = caption.size();
                    (caption.position, Size::new(width, height))
                })
                .collect(),
        }
    }

    pub fn draw(
        &self,
        frame: &mut layered::Frame<'_>,
        camera: Camera,
        bounds: Rectangle,
        selected: bool,
    ) {
        let visible = |a: Point, b: Point, margin: f32| {
            Rectangle {
                x: a.x.min(b.x) - margin,
                y: a.y.min(b.y) - margin,
                width: (a.x - b.x).abs() + margin * 2.,
                height: (a.y - b.y).abs() + margin * 2.,
            }
            .intersects(&Rectangle::with_size(bounds.size()))
        };
        // Keep the existing page-fit policy: the selection box remains visible.
        if !(selected && self.multiple && camera.zoom < 0.12) {
            let scale = camera.zoom.clamp(0.15, 1.);
            for &(a, b) in &self.bonds {
                let a = camera.screen(a, bounds);
                let b = camera.screen(b, bounds);
                if visible(a, b, 3.5 * scale + 2.) {
                    frame.stroke(
                        &Path::line(a, b),
                        Stroke::default()
                            .with_width(7. * scale)
                            .with_color(Color::from_rgba8(19, 135, 116, 0.16)),
                    );
                }
            }
            for &atom in &self.atoms {
                let center = camera.screen(atom, bounds);
                if !visible(center, center, 8. * scale + 3.) {
                    continue;
                }
                let circle = Path::circle(center, 8. * scale);
                if selected {
                    frame.fill(&circle, Color::from_rgba8(19, 135, 116, 0.12));
                }
                frame.stroke(
                    &circle,
                    Stroke::default()
                        .with_width((if selected { 1.8 } else { 1.4 }) * scale.sqrt())
                        .with_color(rgb([19, 135, 116])),
                );
            }
        }
        if selected {
            for &(position, size) in &self.captions {
                let position = camera.screen(position, bounds);
                let size = Size::new(size.width * camera.zoom, size.height * camera.zoom);
                if visible(
                    position,
                    position + iced::Vector::new(size.width, size.height),
                    2.,
                ) {
                    frame.stroke(
                        &Path::rectangle(position, size),
                        Stroke::default().with_color(Color::from_rgb8(20, 130, 112)),
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
