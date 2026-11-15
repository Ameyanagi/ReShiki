//! Two versions of a drawing side by side, with what differs marked: the
//! conflict dialog shows GitHub's and yours, Versions a version and the
//! drawing now. Objects match by id (atoms, text, arrows and graphics share
//! one id space, and ids survive saving), bonds by their two atoms. Both are
//! compared as saved, so a field added later is compared too.
use crate::app::{Message, workspace::muted_text};
use crate::canvas::ComparePreview;
use iced::widget::{Space, column, container, row, text};
use iced::{Element, Length};
use reshiki::document::{Document, Point as World};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// What differs, on one side.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Marks {
    pub(super) ids: HashSet<u64>,
    pub(super) bonds: HashSet<(u64, u64)>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Diff {
    pub(super) left: Marks,
    pub(super) right: Marks,
    /// Something other than an object differs, such as the style or a theme.
    pub(super) settings: bool,
}

fn json(value: &impl serde::Serialize) -> Value {
    serde_json::to_value(value).unwrap_or_default()
}

type Objects = HashMap<(&'static str, u64), (Value, Vec<u64>)>;

/// Every object and link, by kind and id, with the objects a change marks.
fn objects(doc: &Document) -> Objects {
    let mut all = HashMap::new();
    for atom in &doc.atoms {
        all.insert(("atom", atom.id), (json(atom), vec![atom.id]));
    }
    for note in &doc.annotations {
        all.insert(("text", note.id), (json(note), vec![note.id]));
    }
    for arrow in &doc.arrows {
        all.insert(("arrow", arrow.id), (json(arrow), vec![arrow.id]));
    }
    for graphic in &doc.graphics {
        all.insert(("graphic", graphic.id), (json(graphic), vec![graphic.id]));
    }
    for label in &doc.arrow_labels {
        let marks = vec![label.arrow, label.annotation];
        all.insert(("arrow label", label.annotation), (json(label), marks));
    }
    for name in &doc.molecule_names {
        let marks = vec![name.annotation];
        all.insert(("molecule name", name.annotation), (json(name), marks));
    }
    for reaction in &doc.reactions {
        let marks = vec![reaction.arrow];
        all.insert(("reaction", reaction.arrow), (json(reaction), marks));
    }
    all
}

/// Everything that is not an object or a link.
fn settings(doc: &Document) -> Value {
    let mut value = json(doc);
    if let Value::Object(fields) = &mut value {
        for key in [
            "atoms",
            "bonds",
            "annotations",
            "arrows",
            "graphics",
            "arrow_labels",
            "molecule_names",
            "reactions",
            // Neither shows: the file format and the color picker's recents.
            "version",
            "recent_colors",
        ] {
            fields.remove(key);
        }
    }
    value
}

pub(super) fn diff(left: &Document, right: &Document) -> Diff {
    let (objects_left, objects_right) = (objects(left), objects(right));
    let changed = |side: &Objects, other: &Objects| {
        side.iter()
            .filter(|(key, (value, _))| other.get(*key).is_none_or(|(theirs, _)| theirs != value))
            .flat_map(|(_, (_, marks))| marks.iter().copied())
            .collect()
    };
    let bonds = |doc: &Document| -> HashMap<(u64, u64), Value> {
        doc.bonds
            .iter()
            .map(|bond| ((bond.a.min(bond.b), bond.a.max(bond.b)), json(bond)))
            .collect()
    };
    let (bonds_left, bonds_right) = (bonds(left), bonds(right));
    let positions = |doc: &Document| -> HashMap<u64, _> {
        doc.atoms
            .iter()
            .map(|atom| (atom.id, atom.position))
            .collect()
    };
    let (at_left, at_right) = (positions(left), positions(right));
    // A bond is redrawn when either end moved.
    let moved =
        |id: &u64| matches!((at_left.get(id), at_right.get(id)), (Some(a), Some(b)) if a != b);
    let changed_bonds = |side: &HashMap<(u64, u64), Value>, other: &HashMap<_, _>| {
        side.iter()
            .filter(|((a, b), value)| other.get(&(*a, *b)) != Some(*value) || moved(a) || moved(b))
            .map(|(key, _)| *key)
            .collect()
    };
    Diff {
        left: Marks {
            ids: changed(&objects_left, &objects_right),
            bonds: changed_bonds(&bonds_left, &bonds_right),
        },
        right: Marks {
            ids: changed(&objects_right, &objects_left),
            bonds: changed_bonds(&bonds_right, &bonds_left),
        },
        settings: settings(left) != settings(right),
    }
}

/// A box around each marked object, padded, to shade under the drawing.
pub(super) fn boxes(doc: &Document, marks: &Marks) -> Vec<(World, World)> {
    const PAD: f32 = 6.;
    let ids: Vec<Vec<u64>> = marks.ids.iter().map(|id| vec![*id]).collect();
    let mut boxes: Vec<_> = reshiki::scene::selections_bounds(doc, &ids)
        .into_iter()
        .flatten()
        .collect();
    boxes.extend(marks.bonds.iter().filter_map(|(a, b)| {
        let (a, b) = (doc.atom(*a)?.position, doc.atom(*b)?.position);
        Some((
            World::new(a.x.min(b.x), a.y.min(b.y)),
            World::new(a.x.max(b.x), a.y.max(b.y)),
        ))
    }));
    boxes
        .into_iter()
        .map(|(lo, hi)| (lo.offset(-PAD, -PAD), hi.offset(PAD, PAD)))
        .collect()
}

fn extent(doc: &Document) -> (World, World) {
    reshiki::scene::selection_bounds(doc, &doc.all_ids()).unwrap_or_else(|| doc.bounds())
}

/// Two drawings ready to show side by side. The documents are drawn as they
/// are: a highlight colour, or its removal, shows as the difference it is.
#[derive(Debug)]
pub(in crate::app) struct Compare {
    /// `Err`: why there is nothing to show, such as `Deleted on GitHub`.
    pub(super) left: Result<Document, String>,
    pub(super) right: Document,
    pub(super) marks: [Vec<(World, World)>; 2],
    /// Both sides use one camera, so they line up.
    frame: (World, World),
    pub(super) settings: bool,
}

impl Compare {
    pub(super) fn new(left: Result<Document, String>, right: Document) -> Self {
        let (marks, settings, frame) = match &left {
            Ok(left) => {
                let diff = diff(left, &right);
                let (a, b) = (extent(left), extent(&right));
                (
                    [boxes(left, &diff.left), boxes(&right, &diff.right)],
                    diff.settings,
                    (
                        World::new(a.0.x.min(b.0.x), a.0.y.min(b.0.y)),
                        World::new(a.1.x.max(b.1.x), a.1.y.max(b.1.y)),
                    ),
                )
            }
            Err(_) => (Default::default(), false, extent(&right)),
        };
        Self {
            left,
            right,
            marks,
            frame,
            settings,
        }
    }

    /// Both panels under their labels, then what the shading means.
    pub(super) fn view<'a>(&'a self, labels: [&'a str; 2]) -> Element<'a, Message> {
        let panel = |label: &'a str, doc: Result<&'a Document, &'a str>, marks| {
            let body: Element<'a, Message> = match doc {
                Ok(doc) => crate::canvas::layered::canvas(ComparePreview {
                    doc,
                    marks,
                    frame: self.frame,
                })
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
                Err(note) => container(text(note).size(13).style(muted_text))
                    .center(Length::Fill)
                    .into(),
            };
            column![
                text(label).size(13),
                crate::app::inspector::card(container(body).height(Length::Fill)),
            ]
            .spacing(6)
            .width(Length::FillPortion(1))
        };
        let mut legend = String::from("Highlighted: different");
        if self.settings {
            legend.push_str(" · Style or settings changed");
        }
        column![
            row![
                panel(
                    labels[0],
                    self.left.as_ref().map_err(String::as_str),
                    &self.marks[0]
                ),
                panel(labels[1], Ok(&self.right), &self.marks[1]),
            ]
            .spacing(12)
            .height(Length::Fill),
            text(legend).size(12).style(muted_text),
        ]
        .spacing(8)
        .into()
    }

    /// From Versions: a version beside the drawing now.
    pub(super) fn dialog<'a>(&'a self, name: String, label: &'a str) -> Element<'a, Message> {
        let close =
            reshiki::accessibility::button("github-compare-close", "Close", text("Close").size(13))
                .padding([7, 12])
                .style(crate::appearance::secondary)
                .on_press(Message::GitHub(super::Action::Close));
        column![
            text(name).size(18),
            self.view([label, "Now"]),
            row![Space::new().width(Length::Fill), close],
        ]
        .spacing(12)
        .into()
    }
}

#[cfg(test)]
mod tests;
